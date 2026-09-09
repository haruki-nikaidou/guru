//! AMQP event definitions.

use kanau::{RkyvMessageDe, RkyvMessageSer};
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// A canvas has edits that have not been derived yet.
///
/// Published by: every config-affecting mutation (dashboard), plus `Register`,
/// `AckConfig` and `ForgetServerApplied` (workers and dashboard).
/// Consumed by: [`crate::hooks::derive::CanvasDeriver`].
/// Route: exchange `orchestration` (direct), key `canvas_dirty`.
///
/// The payload is only a hint: the canvas generation counter, not this message, is
/// what decides whether a derivation is needed, so a lost message costs latency
/// (until the cron sweep) and never correctness.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct CanvasDirty {
    /// The canvas record key, as `utils::ids::record_key` renders it.
    pub canvas: String,
}

impl AmqpRouting for CanvasDirty {
    const EXCHANGE: &'static str = "orchestration";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "canvas_dirty";
}

impl AmqpMessageSend for CanvasDirty {}
