-- User table definition

DEFINE TABLE users;

DEFINE FIELD name ON TABLE users TYPE string;
DEFINE FIELD email ON TABLE users TYPE string;
DEFINE FIELD hashed_password ON TABLE users TYPE string;
DEFINE FIELD created_at ON TABLE groups TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON TABLE groups TYPE datetime DEFAULT time::now();


-- User group table definition
DEFINE TABLE groups;

DEFINE FIELD name ON TABLE groups TYPE string;
DEFINE FIELD created_at ON TABLE groups TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON TABLE groups TYPE datetime DEFAULT time::now();

-- User permissions table definition
DEFINE TABLE user_permissions;

DEFINE FIELD code ON TABLE groups TYPE string;
DEFINE INDEX code_unique ON TABLE groups COLUMNS code UNIQUE;
DEFINE FIELD created_at ON TABLE groups TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON TABLE groups TYPE datetime DEFAULT time::now();
