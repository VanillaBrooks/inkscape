mod error;
mod object;
mod parse;

use error::*;

pub use object::EncodedImage;

use quick_xml::events::Event;
use quick_xml::name::QName;

use std::io::BufRead;
use std::io::Write;

#[derive(Debug)]
pub struct Inkscape {
    leading_events: Vec<Event<'static>>,
    layers: Vec<Layer>,
    trailing_events: Vec<Event<'static>>,
}

#[derive(Debug)]
pub struct Layer {
    id: String,
    name: String,
    header: Event<'static>,
    content: Vec<object::Object>,
    footer: Event<'static>,
}

impl Layer {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// make a layer visible
    pub fn set_visible(&mut self) {
        let elem = if let Event::Start(elem) = &mut self.header {
            elem
        } else {
            panic!("miss parsed a layer, the header should be Event::Start");
        };

        let mut new_elem = elem.to_owned();
        new_elem.clear_attributes();

        let atts = elem
            .attributes()
            .filter_map(Result::ok)
            .filter(|att| att.key != QName(b"style"));

        new_elem.extend_attributes(atts);
        self.header = Event::Start(new_elem);
    }

    /// make a layer hidden
    pub fn set_hidden(&mut self) {
        let elem = if let Event::Start(elem) = &mut self.header {
            elem
        } else {
            panic!("miss parsed a layer, the header should be Event::Start");
        };

        let mut new_elem = elem.to_owned();
        new_elem.clear_attributes();

        let atts = elem
            .attributes()
            .filter_map(Result::ok)
            .filter(|att| att.key != QName(b"style"))
            .chain(std::iter::once(quick_xml::events::attributes::Attribute {
                key: QName(b"style"),
                value: b"display:none".as_slice().into(),
            }));

        new_elem.extend_attributes(atts);
        self.header = Event::Start(new_elem);
    }

    #[cfg(test)]
    fn eof_group_test(content: Vec<object::Object>) -> Self {
        Self {
            id: "adf".into(),
            name: "adf".into(),
            header: Event::Eof,
            content,
            footer: Event::Eof,
        }
    }
}

/// Export an [`Inkscape`] object to a file
impl Inkscape {
    pub fn write_svg<W: Write>(self, writer: W) -> Result<(), Error> {
        let mut writer = quick_xml::Writer::new(writer);

        for event in self.leading_events {
            writer
                .write_event(&event)
                .map_err(|err| LeadingEvents { err, event })?;
            //.with_context(|| format!("failed to write a leading event: {:?}", event))?;
        }

        for layer in self.layers {
            writer
                .write_event(&layer.header)
                .map_err(|err| LayerHeader {
                    err,
                    header: layer.header,
                })
                .map_err(LayerError::from)?;

            for object in layer.content {
                let event = object.into_event();
                writer
                    .write_event(&event)
                    .map_err(|err| LayerBody { err, object: event })
                    .map_err(LayerError::from)?;
            }

            writer
                .write_event(&layer.footer)
                .map_err(|err| LayerFooter::new(err, layer.footer))
                .map_err(LayerError::from)?;
        }

        for event in self.trailing_events {
            writer
                .write_event(&event)
                .map_err(|err| TrailingEvents::new(err, event))?;
        }

        Ok(())
    }

    pub fn parse_svg<R: BufRead>(reader: R, buffer: &mut Vec<u8>) -> Result<Self, Error> {
        let mut reader = quick_xml::Reader::from_reader(reader);

        let (leading_events, first_group) = parse::leading_events(&mut reader, buffer);

        // read the inner layers
        let (layers, first_trailing) = if let Some(first_group) = first_group {
            let (layers, first_trailing) = parse::layers(&mut reader, buffer, first_group)?;
            (layers, Some(first_trailing))
        } else {
            (vec![], None)
        };

        let trailing_events = if let Some(first_trailing) = first_trailing {
            parse::trailing_events(&mut reader, buffer, first_trailing)
        } else {
            Vec::new()
        };

        let inkscape = Inkscape {
            leading_events,
            layers,
            trailing_events,
        };
        Ok(inkscape)
    }

    pub fn id_to_image(&mut self, id: &str, image: EncodedImage) -> Result<(), MissingId> {
        for layer in &mut self.layers {
            for object in layer.content.iter_mut() {
                match object {
                    object::Object::Rectangle(rect) => {
                        if rect.ident.id == id {
                            let image = rect.set_image(image);
                            *object = object::Object::Image(image);

                            return Ok(());
                        }
                    }
                    object::Object::Image(img) => {
                        if img.ident.id == id {
                            img.update_image(image);

                            return Ok(());
                        }
                    }
                    object::Object::Other(_) => (),
                };
            }
        }

        Err(MissingId::new(id.into()))
    }

    pub fn dimensions(&mut self, id: &str) -> Result<(f64, f64), MissingId> {
        for layer in &self.layers {
            for object in &layer.content {
                match object {
                    object::Object::Rectangle(rect) => {
                        if rect.ident.id == id {
                            return Ok((rect.ident.width, rect.ident.height));
                        }
                    }
                    object::Object::Image(img) => {
                        if img.ident.id == id {
                            return Ok((img.ident.width, img.ident.height));
                        }
                    }
                    object::Object::Other(_) => (),
                };
            }
        }

        Err(MissingId::new(id.into()))
    }

    pub fn object_ids(&self) -> IdIterator<'_> {
        IdIterator::new(&self.layers)
    }

    pub fn get_layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn get_layers_mut(&mut self) -> &mut Vec<Layer> {
        &mut self.layers
    }
}

pub struct IdIterator<'a> {
    curr_group_idx: usize,
    curr_group_object_idx: usize,
    groups: &'a [Layer],
}

impl<'a> IdIterator<'a> {
    pub fn new(groups: &'a [Layer]) -> IdIterator<'a> {
        Self {
            groups,
            curr_group_idx: 0,
            curr_group_object_idx: 0,
        }
    }
}

impl<'a> Iterator for IdIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        // get a current group, or bubble up the `None`
        let group = self.groups.get(self.curr_group_idx)?;

        if let Some(group_object) = group.content.get(self.curr_group_object_idx) {
            // match on the group object to see if this element is a rectangle or image,
            // and therefore contains `Identifier` information we can return from the iterator
            match group_object {
                object::Object::Rectangle(rect) => {
                    self.curr_group_object_idx += 1;
                    Some(&rect.ident.id)
                }
                object::Object::Image(image) => {
                    self.curr_group_object_idx += 1;

                    Some(&image.ident.id)
                }
                object::Object::Other(_) => {
                    // we HAVE a valid object, but since its not an object we normally care
                    // about, we have not parsed the identifiers for it
                    self.curr_group_object_idx += 1;
                    self.next()
                }
            }
        } else {
            // there are no more objects in this layer, go to the next layer
            // group and return anything from there
            self.curr_group_idx += 1;
            self.curr_group_object_idx = 0;

            self.next()
        }
    }
}

#[test]
fn id_iterator() {
    use quick_xml::events::BytesStart;

    use object::{Identifiers, Image, Object, Rectangle};
    let groups = vec![
        Layer::eof_group_test(vec![]),
        Layer::eof_group_test(vec![
            Object::Rectangle(Rectangle::from_ident(Identifiers::zeros_with_id("1"))),
            Object::Rectangle(Rectangle::from_ident(Identifiers::zeros_with_id("2"))),
            Object::Image(Image::from_ident(Identifiers::zeros_with_id("3"))),
        ]),
        Layer::eof_group_test(vec![]),
        Layer::eof_group_test(vec![
            Object::Rectangle(Rectangle::from_ident(Identifiers::zeros_with_id("4"))),
            Object::Other(Event::Empty(BytesStart::new("doesnt_matter"))),
            Object::Other(Event::Empty(BytesStart::new("doesnt_matter2"))),
            Object::Rectangle(Rectangle::from_ident(Identifiers::zeros_with_id("5"))),
        ]),
        Layer::eof_group_test(vec![]),
    ];

    let iter = IdIterator::new(&groups);
    let ids = iter.collect::<Vec<_>>();
    assert_eq!(&["1", "2", "3", "4", "5"], ids.as_slice());
}

#[test]
fn make_layer_hidden() {
    use std::path::PathBuf;

    let path = PathBuf::from("./static/three_layer_hidding.svg");
    let output = PathBuf::from("./static/three_layer_hidding_output.svg");

    let file = std::fs::File::open(&path).unwrap();
    let writer = std::fs::File::create(&output).unwrap();

    let reader = std::io::BufReader::new(file);

    let mut buffer = Vec::new();

    let mut inkscape = Inkscape::parse_svg(reader, &mut buffer).unwrap();

    //dbg!(&inkscape);

    for layer in inkscape.get_layers_mut().iter_mut() {
        dbg!(layer.id(), layer.name());
        if layer.name() == "Layer 2" {
            println!("setting layer 2 hidden");
            layer.set_hidden();
        }

        if layer.name() == "Layer 3" {
            println!("setting layer 3 visible");
            layer.set_visible();
        }
    }

    inkscape.write_svg(writer).unwrap();
}

#[test]
fn many_layers_parse() {
    let path = "./static/julia_python_share_cxx.svg";
    let reader = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    let mut buffer = Vec::new();
    let inkscape = Inkscape::parse_svg(reader, &mut buffer).unwrap();
    let layer_names = inkscape
        .get_layers()
        .iter()
        .map(|layer| layer.name().to_string())
        .collect::<Vec<String>>();

    assert!(layer_names.contains(&"pythoncode".to_string()));
    assert!(layer_names.contains(&"sharedcode".to_string()));
    assert!(layer_names.contains(&"juliacode".to_string()));
    assert!(layer_names.contains(&"julia_bindings".to_string()));
    assert!(layer_names.contains(&"python_bindings".to_string()));

    panic!();
}
