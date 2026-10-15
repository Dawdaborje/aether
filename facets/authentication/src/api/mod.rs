mod login;
mod logout;
mod magic_link;
mod me;
mod methods;
mod oauth;
mod orgs;

pub use login::login;
pub use logout::logout;
pub use magic_link::consume as consume_magic_link;
pub use me::{delete_avatar, get_avatar, me, patch_me, upload_avatar};
pub use methods::methods;
pub use oauth::{oauth_callback, oauth_start};
pub use orgs::{leave_org, list_orgs, switch_org};
