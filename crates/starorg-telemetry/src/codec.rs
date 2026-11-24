//! A tonic codec for [`DynamicMessage`]s: the schema is known only at run time, so there are
//! no generated message types. Encodes any dynamic message; decodes as the message type it
//! was made for (the method's output on a client, its input on a server).

use prost_reflect::prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor};
use tonic::Status;
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};

#[derive(Debug, Clone)]
pub struct DynamicCodec {
    decode_as: MessageDescriptor,
}

impl DynamicCodec {
    /// A codec that decodes messages of type `decode_as`.
    pub fn new(decode_as: MessageDescriptor) -> Self {
        Self { decode_as }
    }
}

impl Codec for DynamicCodec {
    type Encode = DynamicMessage;
    type Decode = DynamicMessage;
    type Encoder = DynamicEncoder;
    type Decoder = DynamicDecoder;

    fn encoder(&mut self) -> Self::Encoder {
        DynamicEncoder
    }

    fn decoder(&mut self) -> Self::Decoder {
        DynamicDecoder(self.decode_as.clone())
    }
}

#[derive(Debug)]
pub struct DynamicEncoder;

impl Encoder for DynamicEncoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn encode(&mut self, item: Self::Item, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        item.encode(dst)
            .map_err(|e| Status::internal(format!("encode: {e}")))
    }
}

#[derive(Debug)]
pub struct DynamicDecoder(MessageDescriptor);

impl Decoder for DynamicDecoder {
    type Item = DynamicMessage;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        DynamicMessage::decode(self.0.clone(), src)
            .map(Some)
            .map_err(|e| Status::internal(format!("decode {}: {e}", self.0.full_name())))
    }
}
