//! SlideBear Kern: Datenmodell, Formatierung, Platzhalter. Ohne GUI- und Netzwerk-Abhängigkeiten.

pub mod assets;
pub mod event;
pub mod export;
pub mod format;
pub mod placeholder;
pub mod presets;
pub mod scene;
pub mod smart;
pub mod sync;

pub use event::{Event, EventFields, EventSource, EventStatus, FieldOverrides, SeriesLink, Template};
pub use scene::{Color, Element, ElementKind, Rect, Scene};
