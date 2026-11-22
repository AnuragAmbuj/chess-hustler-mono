//! hustle-realtime — WebSocket gateway, Redis streams, resume replay, and ticket store (ADR-004, ADR-008).

pub mod codec;
pub mod envelope;
pub mod publisher;
pub mod session;
pub mod ticket;

pub use codec::{read_frame, write_client_frame, write_frame, Frame, Opcode, WsCodecError};
pub use envelope::{ClientCommand, Envelope};
pub use publisher::{EventPublisher, PublisherError};
pub use session::{
    AllowAllAuthorizer, AllowUserAndCallback, ChannelAuthorizer, MoveHandler, SessionError,
    WsSession,
};
pub use ticket::{TicketError, TicketStore};
