//! Organization commands. The work lives in `aether_core::org_admin`, shared with the web app.

pub use aether_core::org_admin::{
    FirstMember, OrganizationRequest, StorageTarget, assign_user, create_organization,
    provision_existing_organization,
};
