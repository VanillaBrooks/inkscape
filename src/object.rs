use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::name::QName;
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
    pub(crate) fn set_image(&mut self, base64_encoded: EncodedImage) -> Image {
        let mut new_element =self.element.to_owned();
        new_element.set_name(b"image")
            .clear_attributes();

        let img_data = quick_xml::events::attributes::Attribute {
            key: QName(b"xlink:href"),
            value: base64_encoded.as_slice().into(),
        };

        let new_atts = self
            .element
            .attributes()
            .filter_map(Result::ok)
            // remove attributes from the iterator that are used for rectangular elements
            .filter(|rect_attribute| rect_attribute.key != QName(b"style"))
            // add on the image data
            .chain(std::iter::once(img_data));

        // update the element, store it in the current element
        // TODO: this updates the underlying element away from `Rectangle`, which may be confusing
        // in the future
        let new_element = new_element.with_attributes(new_atts);

        Image {
            ident: self.ident.clone(),
            element: new_element
        }
    }

    #[cfg(test)]
    pub(crate) fn from_ident(ident: Identifiers) -> Self {
        Self {
            ident,
            element: BytesStart::new("rect"),
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
        //let new_element = quick_xml::events::BytesStart::owned_name(b"image".to_vec());
        let mut new_element = self.element.to_owned();
        new_element.clear_attributes();


        let img_data = quick_xml::events::attributes::Attribute {
            key: QName(b"xlink:href"),
            value: base64_encoded.as_slice().into(),
        };

        let new_atts = self
            .element
            .attributes()
            .filter_map(Result::ok)
            // remove attributes from the iterator that are used for image elements
            .filter(|rect_attribute| rect_attribute.key != QName(b"xlink:href"))
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
            element: BytesStart::new("image"),
        }
    }
}

#[derive(Debug, Clone)]
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
        const WIDTH : QName = QName(b"width");
        const HEIGHT : QName = QName(b"height");
        const ID : QName = QName(b"id");

        let atts = elem
            .attributes()
            .filter_map(Result::ok)
            .filter(|att| att.key == WIDTH || att.key == HEIGHT || att.key == ID);

        let mut width = None;
        let mut height = None;
        let mut id = None;

        for att in atts {
            if att.key == WIDTH {
                let number = String::from_utf8(att.value.to_vec())
                    .map_err(|err| DimensionUtf8::new(err, DimensionOrId::Width))?;

                width = Some(number.parse().map_err(|err| DimensionParse::new(err, DimensionOrId::Width))?);

            } else if att.key == HEIGHT {
                let number = String::from_utf8(att.value.to_vec())
                    .map_err(|err| DimensionUtf8::new(err, DimensionOrId::Height))?;

                height = Some(number.parse().map_err(|err| DimensionParse::new(err, DimensionOrId::Width))?);
            } else if att.key == ID {
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

    dbg!(element);

    let img_path = "./static/10x10_red.png";
    let encoded_bytes = EncodedImage::from_path(img_path).unwrap();

    let bytes = element.as_bytes();
    let reader = std::io::BufReader::new(bytes);
    let mut reader = quick_xml::Reader::from_reader(reader);
    let mut buffer = Vec::new();

    // the second element contains out BytesStart<_>
    let event = reader.read_event_into(&mut buffer).unwrap();

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

    // pull out the element from the structure to ensure that we have changed it how we expected to
    let output_image = image.element.attributes()
        .filter_map(|x| x.ok())
        .find(|att| att.key == QName(b"xlink:href"))
        .unwrap();

    // convert to a string for ease of comparison
    let output_value = String::from_utf8(output_image.value.to_owned().to_vec()).unwrap();

    // ensure that the image has actually changed
    // here QmCC is a string from the end of the above element -
    // if the element was updated correctly then the string should 
    // not be present in the new image data
    assert_eq!(false, output_value.contains("QmCC"));
}

#[test]
fn update_rectangle() {
    let element = r##"<rect
       style="fill:#ff0000;stroke-width:0.665001"
       id="rect286"
       width="85.292282"
       height="48.174355"
       x="38.076923"
       y="16.923077" />
"##;

    let img_path = "./static/10x10_red.png";
    let encoded_bytes = EncodedImage::from_path(img_path).unwrap();

    let bytes = element.as_bytes();
    let reader = std::io::BufReader::new(bytes);
    let mut reader = quick_xml::Reader::from_reader(reader);
    let mut buffer = Vec::new();

    // the second element contains out BytesStart<_>
    let event = reader.read_event_into(&mut buffer).unwrap();

    let object = if let Event::Empty(event) = event {
        super::parse::object(event.into_owned()).unwrap()
    } else {
        panic!("event was {event:?} was /not/ what we expected it to be");
    };

    let mut rect = if let Object::Rectangle(rect) = object {
        rect
    } else {
        panic!("did not parse element as image, this should not happen");
    };

    let image = rect.set_image(encoded_bytes);

    // pull out the element from the structure to ensure that we have changed it how we expected to
    let output_image = image.element.attributes()
        .filter_map(|x| x.ok())
        .find(|att| att.key == QName(b"xlink:href"))
        .unwrap();

    // convert to a string for ease of comparison
    let output_value = String::from_utf8(output_image.value.to_owned().to_vec()).unwrap();

    dbg!(&output_value);

    // ensure that there is an image data section on the new element
    assert_eq!(true, output_value.contains("data:image/png;"));
    assert_eq!(QName(b"image"), image.element.name());
}

#[test]
fn base64_encode_bytes() {
    let img_path = "./static/10x10_green.png";
    EncodedImage::from_path(img_path).unwrap();
}
