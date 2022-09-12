use derive_more::{Constructor, From};
use quick_xml::events::BytesStart;
use std::io;

use std::path::PathBuf;
use std::string::FromUtf8Error;

type StaticEvent = quick_xml::events::Event<'static>;

#[derive(thiserror::Error, Debug, From)]
pub enum Error {
    #[error("{0}")]
    LeadingEvents(LeadingEvents),
    #[error("{0}")]
    Layer(LayerError),
    #[error("{0}")]
    TrailingEvents(TrailingEvents),
    #[error("{0}")]
    ParseLayer(ParseLayer),
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to write leading event `{event:?}` - error: `{err}`")]
pub struct LeadingEvents {
    pub(crate) err: quick_xml::Error,
    pub(crate) event: StaticEvent,
}

#[derive(thiserror::Error, Debug, From)]
#[error("Error writing layer: {0}")]
pub enum LayerError {
    Header(LayerHeader),
    Body(LayerBody),
    Footer(LayerFooter),
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to write header for layer: `{header:?}` - error: `{err}`")]
pub struct LayerHeader {
    pub(crate) err: quick_xml::Error,
    pub(crate) header: StaticEvent,
}

#[derive(thiserror::Error, Debug)]
#[error("Failed to write object in layer body: {object:?} - error: `{err}`")]
pub struct LayerBody {
    pub(crate) err: quick_xml::Error,
    pub(crate) object: StaticEvent,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Failed to write footer for layer: {footer:?} - error: `{err}`")]
pub struct LayerFooter {
    pub(crate) err: quick_xml::Error,
    pub(crate) footer: StaticEvent,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Failed to write trailing event `{event:?}` - error: `{err}`")]
pub struct TrailingEvents {
    pub(crate) err: quick_xml::Error,
    pub(crate) event: StaticEvent,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Id `{id}`was not found in document")]
pub struct MissingId {
    pub(crate) id: String,
}

#[derive(thiserror::Error, Debug, From)]
pub enum ParseLayer {
    #[error("failed to parse layer: `{0}`")]
    MissingLayerEnd(MissingLayerEnd),
    #[error("failed to parse layer: `{0}`")]
    ParseObject(ParseObject),
    #[error("failed to parse layer: `{0}`")]
    MissingLayerName(MissingLayerName),
    #[error("failed to parse layer: `{0}`")]
    MissingLayerId(MissingLayerId),
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("missing `id` attribute for <g> attribute of layer, maybe it was not UTF8? `{element:?}`")]
pub struct MissingLayerId {
    element: BytesStart<'static>,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Failed to parse object in layer {layer_name}: {error}")]
pub struct ParseObject {
    pub(crate) error: IdentifierError,
    pub(crate) layer_name: String,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Missing end of group attribute for layer {layer_name}")]
pub struct MissingLayerEnd {
    pub(crate) layer_name: String,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Layer name was missing or not UTF8 for event: {event:?}")]
pub struct MissingLayerName {
    event: BytesStart<'static>,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Failed to parse `{dimension}` parameter to utf8 string; error: `{error}`")]
pub struct DimensionUtf8 {
    error: FromUtf8Error,
    dimension: DimensionOrId,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("Failed to parse `{dimension}` parameter to utf8 string; error: `{error}`")]
pub struct DimensionParse {
    error: std::num::ParseFloatError,
    dimension: DimensionOrId,
}

#[derive(Debug, derive_more::Display)]
pub enum DimensionOrId {
    #[display(fmt = "width")]
    Width,
    #[display(fmt = "height")]
    Height,
    #[display(fmt = "id")]
    Id,
}

#[derive(thiserror::Error, Debug, From)]
pub enum IdentifierError {
    #[error("Failed to parse identifier: `{0}`")]
    DimensionUtf8(DimensionUtf8),
    #[error("Failed to parse identifier: `{0}`")]
    DimensionParse(DimensionParse),
    #[error("Failed to parse identifier: `{0}`")]
    MissingObjectIdentifier(MissingObjectIdentifier),
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("One of width ({width:?}) / height ({height:?}) / id ({id:?} was missing for element {element:?}")]
pub struct MissingObjectIdentifier {
    element: BytesStart<'static>,
    width: Option<f64>,
    height: Option<f64>,
    id: Option<String>,
}

#[derive(thiserror::Error, Debug, From)]
pub enum EncodingError {
    #[error("Error while encoding image: `{0}`")]
    OpenFile(OpenFile),
    #[error("Error while encoding image: `{0}`")]
    ReadBytes(ReadBytes),
    #[error("Error while encoding image: `{0}`")]
    UnknownMime(UnknownMime),
    #[error("Error while encoding image: `{0}`")]
    WrongEncoding(WrongEncoding),
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error("failed to open file at {}; error: {error}", "path.display()")]
pub struct OpenFile {
    error: io::Error,
    path: PathBuf,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error(
    "failed to read bytes of file {} after it was opened; error: {error}",
    "path.display()"
)]
pub struct ReadBytes {
    error: io::Error,
    path: PathBuf,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error(
    "image at path {} has an unknown mime type. figure_second only handles PNG encoded images",
    "path.display()"
)]
pub struct UnknownMime {
    path: PathBuf,
}

#[derive(thiserror::Error, Debug, Constructor)]
#[error(
    "image at path {} is not PNG encoded. Images must be png encoded currently",
    "path.display()"
)]
pub struct WrongEncoding {
    path: PathBuf,
}
