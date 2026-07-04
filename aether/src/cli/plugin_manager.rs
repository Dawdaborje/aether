use std::process::exit;

#[derive(Default, Debug)]
pub enum PluginSource {
    #[default]
    ConfigFile,
    Database,
}

pub async fn get_plugins(source: PluginSource) {
    match source {
        PluginSource::ConfigFile => {
            log::info!("Implemented");
        }
        PluginSource::Database => {
            log::info!("Not implemented yet");
            exit(0);
        }
    }
}
