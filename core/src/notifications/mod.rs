pub mod channels;
pub mod error;
pub mod repo;
pub mod service;

pub use error::{DeliveryFailure, NotificationError};
pub use service::{
    ChannelUpdate, ConfigMerge, NewChannel, NotificationEvent, NotificationService, validate_config,
};
