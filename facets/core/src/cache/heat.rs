//! Per-org Mantle heat ledger (durable JSON under app_dir).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::labels::CacheLabel;
use crate::app_dir::AppDir;

const HEAT_VERSION: u32 = 1;
const DEFAULT_HALF_LIFE_SECS: f64 = 12.0 * 3600.0;
const MAX_PLUGINS: usize = 50;
const MAX_MODELS: usize = 50;
const MAX_TOP_KEYS: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatPlugin {
    pub plugin: String,
    pub score: f64,
    pub hits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatModel {
    pub plugin: String,
    pub model: String,
    pub kind: String,
    pub score: f64,
    pub hits: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatTopKey {
    pub plugin: String,
    pub key: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatFile {
    pub v: u32,
    pub org: String,
    pub updated_at: String,
    pub window_secs: u64,
    pub plugins: Vec<HeatPlugin>,
    pub models: Vec<HeatModel>,
    pub top_keys: Vec<HeatTopKey>,
}

/// Key tallies are split over this many locks per organization.
const KEY_SHARDS: usize = 16;

type KeyTally = HashMap<(String, String), f64>;

/// Plugin and model tallies: few entries, touched on every hit.
#[derive(Debug)]
struct Core {
    plugins: HashMap<String, (f64, u64)>,
    models: HashMap<(String, String, String), (f64, u64)>,
    last_decay: Instant,
}

/// One organization's heat. Its own locks: another organization's traffic never waits on it,
/// and key tallies (the large map) are spread over [`KEY_SHARDS`] locks.
#[derive(Debug)]
struct OrgHeat {
    core: Mutex<Core>,
    /// `(plugin, user_key)` → score, sharded by hash. Keyed by plugin too: two plugins may
    /// share a key.
    keys: Vec<Mutex<KeyTally>>,
    dirty: AtomicBool,
    /// The persisted file has been merged in (once per process per org).
    loaded: AtomicBool,
}

impl Default for OrgHeat {
    fn default() -> Self {
        Self {
            core: Mutex::new(Core {
                plugins: HashMap::new(),
                models: HashMap::new(),
                last_decay: Instant::now(),
            }),
            keys: (0..KEY_SHARDS).map(|_| Mutex::new(HashMap::new())).collect(),
            dirty: AtomicBool::new(false),
            loaded: AtomicBool::new(false),
        }
    }
}

fn shard_of(plugin: &str, key: &str) -> usize {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    plugin.hash(&mut hasher);
    key.hash(&mut hasher);
    (hasher.finish() as usize) % KEY_SHARDS
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug)]
pub struct HeatLedger {
    app_dir: PathBuf,
    half_life_secs: f64,
    /// The map is only written when an organization is first seen; every touch takes the read
    /// side, then works on that organization's own locks.
    orgs: RwLock<HashMap<String, Arc<OrgHeat>>>,
}

impl HeatLedger {
    pub fn new(app_dir: impl Into<PathBuf>, half_life_secs: Option<f64>) -> Self {
        Self {
            app_dir: app_dir.into(),
            half_life_secs: half_life_secs.unwrap_or(DEFAULT_HALF_LIFE_SECS),
            orgs: RwLock::new(HashMap::new()),
        }
    }

    fn existing(&self, org: &str) -> Option<Arc<OrgHeat>> {
        self.orgs
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(org)
            .cloned()
    }

    fn org(&self, org: &str) -> Arc<OrgHeat> {
        if let Some(found) = self.existing(org) {
            return found;
        }
        let mut map = self.orgs.write().unwrap_or_else(|e| e.into_inner());
        Arc::clone(map.entry(org.to_string()).or_default())
    }

    pub fn touch(&self, org: &str, plugin: &str, label: &CacheLabel, user_key: &str) {
        if org.is_empty() || plugin.is_empty() {
            return;
        }
        let heat = self.org(org);
        let decay = {
            let mut core = lock(&heat.core);
            let decay = Self::decay_factor(&mut core, self.half_life_secs);
            if let Some(factor) = decay {
                for (_, (score, _)) in core.plugins.iter_mut() {
                    *score *= factor;
                }
                for (_, (score, _)) in core.models.iter_mut() {
                    *score *= factor;
                }
            }
            {
                let slot = core.plugins.entry(plugin.to_string()).or_insert((0.0, 0));
                slot.0 += 1.0;
                slot.1 += 1;
            }
            if let Some(model) = label.model.as_deref() {
                let key = (
                    plugin.to_string(),
                    model.to_string(),
                    label.kind.as_str().to_string(),
                );
                let slot = core.models.entry(key).or_insert((0.0, 0));
                slot.0 += 1.0;
                slot.1 += 1;
            }
            decay
        };
        if let Some(factor) = decay {
            for shard in &heat.keys {
                for (_, score) in lock(shard).iter_mut() {
                    *score *= factor;
                }
            }
        }
        {
            let mut shard = lock(&heat.keys[shard_of(plugin, user_key)]);
            *shard
                .entry((plugin.to_string(), user_key.to_string()))
                .or_insert(0.0) += 1.0;
            if shard.len() > MAX_TOP_KEYS * 4 / KEY_SHARDS {
                Self::prune_shard(&mut shard);
            }
        }
        heat.dirty.store(true, Ordering::Relaxed);
    }

    /// Keep only the hottest keys so the in-memory map stays bounded between flushes.
    fn prune_shard(shard: &mut KeyTally) {
        let mut scored: Vec<((String, String), f64)> = shard.drain().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(MAX_TOP_KEYS * 2 / KEY_SHARDS);
        *shard = scored.into_iter().collect();
    }

    /// The factor every score is due to be multiplied by, at most once a minute.
    fn decay_factor(core: &mut Core, half_life_secs: f64) -> Option<f64> {
        let elapsed = core.last_decay.elapsed().as_secs_f64();
        if elapsed < 60.0 {
            return None;
        }
        core.last_decay = Instant::now();
        Some(0.5_f64.powf(elapsed / half_life_secs.max(1.0)))
    }

    pub fn snapshot(&self, org: &str) -> Option<HeatFile> {
        let heat = self.existing(org)?;
        Some(Self::to_file(org, &heat))
    }

    pub fn known_orgs(&self) -> Vec<String> {
        self.orgs
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }

    pub fn priority(&self, org: &str, plugin: &str, model: Option<&str>) -> f64 {
        let Some(heat) = self.existing(org) else {
            return 0.0;
        };
        let core = lock(&heat.core);
        let mut score = core.plugins.get(plugin).map(|(s, _)| *s).unwrap_or(0.0);
        if let Some(model) = model {
            for kind in ["row", "list", "agg"] {
                if let Some((s, _)) =
                    core.models
                        .get(&(plugin.to_string(), model.to_string(), kind.to_string()))
                {
                    score += *s;
                }
            }
        }
        score
    }

    fn to_file(org: &str, heat: &OrgHeat) -> HeatFile {
        let (plugins, models) = {
            let core = lock(&heat.core);
            let plugins: Vec<HeatPlugin> = core
                .plugins
                .iter()
                .map(|(plugin, (score, hits))| HeatPlugin {
                    plugin: plugin.clone(),
                    score: *score,
                    hits: *hits,
                })
                .collect();
            let models: Vec<HeatModel> = core
                .models
                .iter()
                .map(|((plugin, model, kind), (score, hits))| HeatModel {
                    plugin: plugin.clone(),
                    model: model.clone(),
                    kind: kind.clone(),
                    score: *score,
                    hits: *hits,
                })
                .collect();
            (plugins, models)
        };
        let mut plugins = plugins;
        plugins.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        plugins.truncate(MAX_PLUGINS);

        let mut models = models;
        models.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        models.truncate(MAX_MODELS);

        let mut top_keys: Vec<HeatTopKey> = Vec::new();
        for shard in &heat.keys {
            top_keys.extend(lock(shard).iter().map(|((plugin, key), score)| HeatTopKey {
                plugin: plugin.clone(),
                key: key.clone(),
                score: *score,
            }));
        }
        top_keys.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        top_keys.truncate(MAX_TOP_KEYS);

        let updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| {
                chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0)
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".into())
            })
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());

        HeatFile {
            v: HEAT_VERSION,
            org: org.to_string(),
            updated_at,
            window_secs: 86400,
            plugins,
            models,
            top_keys,
        }
    }

    /// Merge the persisted heat of `org` into memory, once per process. Returns `true` when this
    /// call did the load. Live counts that accumulated before the load are kept and added to.
    pub fn ensure_loaded(&self, org: &str) -> bool {
        let heat = self.org(org);
        if heat.loaded.swap(true, Ordering::AcqRel) {
            return false;
        }
        if let Err(error) = self.merge_file(org) {
            log::debug!("mantle heat load for {org}: {error}");
        }
        true
    }

    pub fn is_loaded(&self, org: &str) -> bool {
        self.existing(org)
            .is_some_and(|h| h.loaded.load(Ordering::Acquire))
    }

    pub fn load_org(&self, org: &str) -> Result<Option<HeatFile>, std::io::Error> {
        self.org(org).loaded.store(true, Ordering::Release);
        self.merge_file(org)
    }

    fn merge_file(&self, org: &str) -> Result<Option<HeatFile>, std::io::Error> {
        let path = heat_path(&self.app_dir, org)?;
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)?;
        let file: HeatFile = serde_json::from_str(&raw)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let heat = self.org(org);
        {
            let mut core = lock(&heat.core);
            for p in &file.plugins {
                let slot = core.plugins.entry(p.plugin.clone()).or_insert((0.0, 0));
                slot.0 += p.score;
                slot.1 += p.hits;
            }
            for m in &file.models {
                let slot = core
                    .models
                    .entry((m.plugin.clone(), m.model.clone(), m.kind.clone()))
                    .or_insert((0.0, 0));
                slot.0 += m.score;
                slot.1 += m.hits;
            }
            core.last_decay = Instant::now();
        }
        for k in &file.top_keys {
            let mut shard = lock(&heat.keys[shard_of(&k.plugin, &k.key)]);
            *shard
                .entry((k.plugin.clone(), k.key.clone()))
                .or_insert(0.0) += k.score;
        }
        Ok(Some(file))
    }

    pub fn flush_dirty(&self) -> Result<usize, std::io::Error> {
        let dirty_orgs: Vec<String> = self
            .orgs
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, h)| h.dirty.load(Ordering::Relaxed))
            .map(|(org, _)| org.clone())
            .collect();
        let mut written = 0;
        for org in dirty_orgs {
            self.flush_org(&org)?;
            written += 1;
        }
        Ok(written)
    }

    pub fn flush_org(&self, org: &str) -> Result<(), std::io::Error> {
        let Some(heat) = self.existing(org) else {
            return Ok(());
        };
        let decay = Self::decay_factor(&mut lock(&heat.core), self.half_life_secs);
        if let Some(factor) = decay {
            let mut core = lock(&heat.core);
            for (_, (score, _)) in core.plugins.iter_mut() {
                *score *= factor;
            }
            for (_, (score, _)) in core.models.iter_mut() {
                *score *= factor;
            }
            drop(core);
            for shard in &heat.keys {
                for (_, score) in lock(shard).iter_mut() {
                    *score *= factor;
                }
            }
        }
        heat.dirty.store(false, Ordering::Relaxed);
        let file = Self::to_file(org, &heat);
        let dir = mantle_dir(&self.app_dir, org)?;
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("heat.json");
        let tmp = dir.join("heat.json.tmp");
        let body = serde_json::to_vec_pretty(&file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn flush_interval() -> Duration {
        Duration::from_secs(30)
    }
}

fn mantle_dir(app_dir: &Path, org: &str) -> Result<PathBuf, std::io::Error> {
    let app = AppDir::new(app_dir);
    app.org_mantle_dir(org)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))
}

fn heat_path(app_dir: &Path, org: &str) -> Result<PathBuf, std::io::Error> {
    let app = AppDir::new(app_dir);
    app.org_mantle_heat_path(org)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn concurrent_touches_across_orgs_are_all_counted() {
        let dir = std::env::temp_dir().join(format!("mantle-heat-conc-{}", std::process::id()));
        let heat = Arc::new(HeatLedger::new(&dir, Some(3600.0)));
        let mut threads = Vec::new();
        for t in 0..8 {
            let heat = Arc::clone(&heat);
            threads.push(std::thread::spawn(move || {
                let org = format!("org{}", t % 2);
                for i in 0..500 {
                    let key = format!("row:contact:{}", i % 40);
                    let label = CacheLabel::parse(&format!("plugin:{org}:crm::{key}"));
                    heat.touch(&org, "crm", &label, &key);
                }
            }));
        }
        for t in threads {
            t.join().unwrap();
        }
        for org in ["org0", "org1"] {
            let snap = heat.snapshot(org).unwrap();
            assert_eq!(snap.plugins[0].hits, 4 * 500, "{org}");
            assert!(snap.top_keys.len() <= MAX_TOP_KEYS);
            assert_eq!(snap.top_keys.len(), 40);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
