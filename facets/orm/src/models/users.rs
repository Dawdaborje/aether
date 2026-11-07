use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct UserModel {
    pub username: String,
    pub password: String,
    pub email: String,
}
