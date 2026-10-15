-- Filename under app_dir/users/<id>/ for the profile photo (avatar.png, …).
-- Public URL is served as /api/auth/me/avatar (see avatar_url on /api/auth/me).

DEFINE FIELD IF NOT EXISTS avatar_file ON TABLE users TYPE option<string | NONE>;
