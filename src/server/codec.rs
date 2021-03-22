use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use bytes::{BufMut, BytesMut};
use ergvein_protocol::message::*;
use std::io;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct MessageCodec;

impl Default for MessageCodec {
    fn default() -> MessageCodec {
        MessageCodec
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = ergvein_protocol::message::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Message>, Error> {
        if !buf.is_empty() {
            match deserialize_partial::<Message>(buf) {
                Err(Error::Io(ref err)) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
                Err(err) => return Err(err),
                Ok((message, index)) => {
                    let _ = buf.split_to(index);
                    Ok(Some(message))
                }
            }
        } else {
            Ok(None)
        }
    }
}

impl Encoder<Message> for MessageCodec {
    type Error = ergvein_protocol::message::Error;

    fn encode(&mut self, data: Message, buf: &mut BytesMut) -> Result<(), Error> {
        let bytes = serialize(&data);
        buf.reserve(bytes.len());
        buf.put(bytes.as_slice());
        Ok(())
    }
}
