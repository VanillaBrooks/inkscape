use super::object;
use super::Layer;

use super::error::*;

use quick_xml::events::BytesStart;
use quick_xml::events::Event;
use quick_xml::name::QName;

use std::io::BufRead;

pub(crate) fn leading_events<R: BufRead>(
    reader: &mut quick_xml::Reader<R>,
    buffer: &mut Vec<u8>,
) -> (Vec<Event<'static>>, Option<BytesStart<'static>>) {
    let mut out = Vec::new();

    while let Ok(event) = reader.read_event_into(buffer) {
        let event = event.into_owned();

        if let Event::Start(element) = event {
            // if the name is starts a <g> tag then we
            // know that we are out of the leading events and are now in
            // the layer parsing, we need to return
            if element.name() == QName(b"g") {
                return (out, Some(element))
            } else {
                // we had a Event::Start(_) but it was not a starting
                // event for <g>, so lets just put it back into Event::Start(_)
                // and move on
                out.push(Event::Start(element));
            }
        } else {
            out.push(event);
        }
    }

    (out, None)
}

pub(crate) fn trailing_events<R: BufRead>(
    reader: &mut quick_xml::Reader<R>,
    buffer: &mut Vec<u8>,
    first_trailing_event: Event<'static>,
) -> Vec<Event<'static>> {
    let mut out = Vec::new();

    out.push(first_trailing_event);
    while let Ok(event) = reader.read_event_into(buffer) {
        if let Event::Eof = event {
            break;
        } else {
            out.push(event.into_owned())
        }
    }

    out
}

pub(crate) fn layers<R: BufRead>(
    reader: &mut quick_xml::Reader<R>,
    buffer: &mut Vec<u8>,
    first_layer_start: BytesStart<'static>,
) -> Result<(Vec<Layer>, Event<'static>), ParseLayer> {
    let mut out = Vec::new();

    let first_group = group(first_layer_start, reader, buffer)?;
    out.push(first_group);

    while let Ok(event) = reader.read_event_into(buffer) {
        let event = event.into_owned();

        if let Event::Start(element) = event {
            if element.name() == QName(b"g") {
                // TODO: why are these things below commented, and why does
                // this work?

                // we are at the first layer event, leave this function
                //return Ok((out, Some(event)))

                let grp = group(element, reader, buffer)?;
                out.push(grp);

            }
        } else {
            return Ok((out, event));
        }
    }

    // only happens if our while let Ok(_) = loop ends with an error 
    // which requires some malformed xml
    panic!("finished / errored on reading xml elements from inkscape document without returning correctly. Your inkscape document likely contains malformed xml");
}

/// parse all the contents (including header tag) of `<g> ... </g>` elements
pub(crate) fn group<R: BufRead>(
    start_event: BytesStart<'static>,
    reader: &mut quick_xml::Reader<R>,
    buffer: &mut Vec<u8>,
) -> Result<Layer, ParseLayer> {
    let mut content = Vec::new();

    let mut footer = None;

    while let Ok(event) = reader.read_event_into(buffer) {
        let event = event.into_owned();

        match event {
            Event::Empty(xml_object) => {
                // parse the object
                let object = object(xml_object)
                    .map_err(|err| {
                        let layer_name = layer_name(&start_event).unwrap();
                        ParseObject::new(err, layer_name)
                    })?;

                content.push(object);
            }
            Event::End(end) => {
                footer = Some(Event::End(end));
                break;
            }
            other_event => {
                content.push(object::Object::Other(other_event));
            }
        }
    }

    let footer = if let Some(inner_footer) = footer {
        inner_footer
    } else {
        let name = layer_name(&start_event)?;
        return Err(MissingLayerEnd::new(name).into());
    };

    let grp = Layer {
        header: Event::Start(start_event),
        content,
        footer,
    };

    Ok(grp)
}

/// map an element inside <g>... </g> to a `Object` that may be adjusted
/// by the user
pub(crate) fn object(element: BytesStart<'static>) -> Result<object::Object, IdentifierError> {
    let obj = match element.name() {
        QName(b"image") => {
            // parse as an image
            let ident = object::Identifiers::from_elem(&element)?;

            object::Object::Image(object::Image { ident, element })
        }
        QName(b"rect") => {
            // parse as a rectangle
            let ident = object::Identifiers::from_elem(&element)?;

            object::Object::Rectangle(object::Rectangle { ident, element })
        }
        _unknown => object::Object::Other(Event::Empty(element)),
    };

    Ok(obj)
}

fn layer_name(layer_start_event: &BytesStart<'static>) -> Result<String, MissingLayerName> {
    let (_, name_id) = layer_start_event
        .attributes()
        .into_iter()
        .filter_map(|x| x.ok())
        .map(|att| (att.key, att.value))
        .find(|(key, _)| key ==  &QName(b"id"))
        .ok_or_else(|| MissingLayerName::new(layer_start_event.clone()))?;

    let out = String::from_utf8(name_id.to_vec())
        .map_err(|_| MissingLayerName::new(layer_start_event.clone()))?;

    Ok(out)
}
