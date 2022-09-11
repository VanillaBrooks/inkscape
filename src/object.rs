use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use std::io::Read;

use super::error::*;

use std::fmt::Write as _;

use std::path::Path;

#[derive(Debug)]
pub(crate) enum Object {
    Rectangle(Rectangle),
    Image(Image),
    /// other does not necessarily have to be a image or geometrical event,
    /// it could also be spacing events
    Other(Event<'static>),
}

impl Object {
    pub(crate) fn into_event(self) -> Event<'static> {
        match self {
            Self::Rectangle(rect) => Event::Empty(rect.element),
            Self::Image(image) => Event::Empty(image.element),
            Self::Other(object) => object,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Rectangle {
    pub ident: Identifiers,
    pub(crate) element: BytesStart<'static>,
}

impl Rectangle {
    pub(crate) fn set_image(&mut self, base64_encoded: EncodedImage) {
        let new_element = quick_xml::events::BytesStart::owned_name(b"image".to_vec());

        let img_data = quick_xml::events::attributes::Attribute {
            key: b"xlink:href",
            value: base64_encoded.as_slice().into(),
        };

        let new_atts = self
            .element
            .attributes()
            .filter_map(Result::ok)
            // remove attributes from the iterator that are used for rectangular elements
            .filter(|rect_attribute| rect_attribute.key != b"style")
            // add on the image data
            .chain(std::iter::once(img_data));

        // update the element, store it in the current element
        // TODO: this updates the underlying element away from `Rectangle`, which may be confusing
        // in the future
        let new_element = new_element.with_attributes(new_atts);
        self.element = new_element;
    }

    #[cfg(test)]
    pub(crate) fn from_ident(ident: Identifiers) -> Self {
        Self {
            ident,
            element: BytesStart::owned_name(b"rect".to_vec()),
        }
    }
}

#[derive(Debug)]
/// an image with base64 encoding in inkscape
///
/// actual content of the image is stored in the xlink:href attribute
/// of the element field.
pub(crate) struct Image {
    pub ident: Identifiers,
    pub(crate) element: BytesStart<'static>,
}

impl Image {
    pub(crate) fn update_image(&mut self, base64_encoded: EncodedImage) {
        let new_element = quick_xml::events::BytesStart::owned_name(b"image".to_vec());

        let img_data = quick_xml::events::attributes::Attribute {
            key: b"xlink:href",
            value: base64_encoded.as_slice().into(),
        };

        let new_atts = self
            .element
            .attributes()
            .filter_map(Result::ok)
            // remove attributes from the iterator that are used for image elements
            .filter(|rect_attribute| rect_attribute.key != b"xlink:href")
            // add on the image data
            .chain(std::iter::once(img_data));

        // update the element, store it in the current element
        // TODO: this updates the underlying element away from `Rectangle`, which may be confusing
        // in the future
        let new_element = new_element.with_attributes(new_atts);
        self.element = new_element;
    }

    #[cfg(test)]
    pub(crate) fn from_ident(ident: Identifiers) -> Self {
        Self {
            ident,
            element: BytesStart::owned_name(b"image".to_vec()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Identifiers {
    pub(crate) id: String,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Identifiers {
    #[cfg(test)]
    pub(crate) fn zeros_with_id<T: Into<String>>(id: T) -> Self {
        Self {
            id: id.into(),
            width: 0.0,
            height: 0.0,
        }
    }

    pub(crate) fn from_elem(elem: &BytesStart<'static>) -> Result<Self, IdentifierError> {
        let atts = elem
            .attributes()
            .filter_map(Result::ok)
            .filter(|att| att.key == b"width" || att.key == b"height" || att.key == b"id");

        let mut width = None;
        let mut height = None;
        let mut id = None;

        for att in atts {
            if att.key == b"width" {
                let number = String::from_utf8(att.value.to_vec())
                    .map_err(|err| DimensionUtf8::new(err, DimensionOrId::Width))?;

                width = Some(number.parse().map_err(|err| DimensionParse::new(err, DimensionOrId::Width))?);

            } else if att.key == b"height" {
                let number = String::from_utf8(att.value.to_vec())
                    .map_err(|err| DimensionUtf8::new(err, DimensionOrId::Height))?;

                height = Some(number.parse().map_err(|err| DimensionParse::new(err, DimensionOrId::Width))?);
            } else if att.key == b"id" {
                let id_utf8 = String::from_utf8(att.value.to_vec())
                    .map_err(|err| DimensionUtf8::new(err, DimensionOrId::Id))?;
                id = Some(id_utf8)
            }
        }

        let out = match (width,height,id)  {
            (Some(width), Some(height), Some(id)) => {
                Identifiers {id, width, height }
            }
            (w, h, id) => return Err(MissingObjectIdentifier::new(elem.clone(), w, h, id).into())
        };

        Ok(out)
    }
}

pub struct EncodedImage {
    // base64 encoded bytes with Inkscape mime type prefixed
    base64_bytes: Vec<u8>,
}

impl EncodedImage {
    fn as_slice(&self) -> &[u8] {
        self.base64_bytes.as_slice()
    }

    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Self, EncodingError> {
        let path = path.as_ref();

        let mut file = std::fs::File::open(&path)
            .map_err(|err| OpenFile::new(err, path.to_owned()))?;

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|err| ReadBytes::new(err, path.to_owned()))?;

        let format = image::guess_format(&bytes)
            .map_err(|_| UnknownMime::new(path.to_owned()))?;

        if !matches!(format, image::ImageFormat::Png) {
            return Err(WrongEncoding::new(path.to_owned()).into())
        }

        let mut base64_buf = String::with_capacity(bytes.len());

        // add some inkscape MIME data to the start of the output
        write!(base64_buf, "data:image/png;base64,").unwrap();

        // encode the bytes as base64
        base64::encode_config_buf(bytes, base64::STANDARD, &mut base64_buf);

        Ok(Self {
            base64_bytes: base64_buf.into_bytes(),
        })
    }
}

#[test]
fn update_image() {
    let element = r##"<image
       width="0.84666669"
       height="0.84666669"
       preserveAspectRatio="none"
       xlink:href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAoAAAAKCAIAAAACUFjqAAABhGlDQ1BJQ0MgcHJvZmlsZQAAKJF9
kT1Iw0AcxV9bpSItIlYQcchQnSyIiuimVShChVArtOpgcukXNGlIUlwcBdeCgx+LVQcXZ10dXAVB
8APE0clJ0UVK/F9SaBHjwXE/3t173L0D/PUyU82OMUDVLCOViAuZ7KoQfEUQ/ejFDMISM/U5UUzC
c3zdw8fXuxjP8j735wgrOZMBPoF4lumGRbxBPLVp6Zz3iSOsKCnE58SjBl2Q+JHrsstvnAsO+3lm
xEin5okjxEKhjeU2ZkVDJZ4kjiqqRvn+jMsK5y3OarnKmvfkLwzltJVlrtMcQgKLWIIIATKqKKEM
CzFaNVJMpGg/7uEfdPwiuWRylcDIsYAKVEiOH/wPfndr5ifG3aRQHOh8se2PYSC4CzRqtv19bNuN
EyDwDFxpLX+lDkx/kl5radEjoGcbuLhuafIecLkDDDzpkiE5UoCmP58H3s/om7JA3y3Qveb21tzH
6QOQpq6SN8DBITBSoOx1j3d3tff275lmfz+OwHKyncxEXAAAAAlwSFlzAAAuIwAALiMBeKU/dgAA
AAd0SU1FB+YHFRE6EhLaT/QAAAAZdEVYdENvbW1lbnQAQ3JlYXRlZCB3aXRoIEdJTVBXgQ4XAAAA
FUlEQVQY02MMaBRnwA2YGPCCkSoNACS6APwkkpJNAAAAAElFTkSuQmCC
"
       id="image356"
       x="36.497185"
       y="76.012566" />
"##;

    let img_path = "./static/10x10_green.png";
    let encoded_bytes = EncodedImage::from_path(img_path).unwrap();

    let bytes = element.as_bytes();
    let reader = std::io::BufReader::new(bytes);
    let mut reader = quick_xml::Reader::from_reader(reader);
    let mut buffer = Vec::new();

    // for some reason the first item out is always useless
    let _first_event = reader.read_event(&mut buffer).unwrap();
    // the second element contains out BytesStart<_>
    let event = reader.read_event(&mut buffer).unwrap();

    let object = if let Event::Empty(event) = event {
        super::parse::object(event.into_owned()).unwrap()
    } else {
        panic!("event was {event:?} was /not/ what we expected it to be");
    };

    let mut image = if let Object::Image(img) = object {
        img
    } else {
        panic!("did not parse element as image, this should not happen");
    };

    image.update_image(encoded_bytes);

    dbg!(&image);

    //panic!();
}

#[test]
fn base64_encode_bytes() {
    let img_path = "./static/10x10_green.png";
    EncodedImage::from_path(img_path).unwrap();
}
