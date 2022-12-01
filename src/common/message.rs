/*
 * Copyright (c) 2020-2022, Stalwart Labs Ltd.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

use mail_parser::{parsers::MessageStream, HeaderValue};
use sha1::Sha1;
use sha2::Sha256;

use crate::{
    arc,
    dkim::{self, HashAlgorithm},
    AuthenticatedMessage,
};

use super::headers::{AuthenticatedHeader, Header, HeaderParser};

impl<'x> AuthenticatedMessage<'x> {
    pub fn parse(raw_message: &'x [u8]) -> Option<Self> {
        let mut message = AuthenticatedMessage {
            headers: Vec::new(),
            from: Vec::new(),
            body: b"",
            body_hashes: Vec::new(),
            dkim_headers: Vec::new(),
            ams_headers: Vec::new(),
            as_headers: Vec::new(),
            aar_headers: Vec::new(),
        };

        let mut headers = HeaderParser::new(raw_message);
        let mut has_arc_errors = false;

        for (header, value) in &mut headers {
            let name = match header {
                AuthenticatedHeader::Ds(name) => {
                    let signature = dkim::Signature::parse(value);
                    if let Ok(signature) = &signature {
                        let ha = HashAlgorithm::from(signature.a);
                        if !message
                            .body_hashes
                            .iter()
                            .any(|(c, h, l, _)| c == &signature.cb && h == &ha && l == &signature.l)
                        {
                            message
                                .body_hashes
                                .push((signature.cb, ha, signature.l, Vec::new()));
                        }
                    }
                    message
                        .dkim_headers
                        .push(Header::new(name, value, signature));
                    name
                }
                AuthenticatedHeader::Aar(name) => {
                    let results = arc::Results::parse(value);
                    if !has_arc_errors {
                        has_arc_errors = results.is_err();
                    }
                    message.aar_headers.push(Header::new(name, value, results));
                    name
                }
                AuthenticatedHeader::Ams(name) => {
                    let signature = arc::Signature::parse(value);

                    if let Ok(signature) = &signature {
                        let ha = HashAlgorithm::from(signature.a);
                        if !message
                            .body_hashes
                            .iter()
                            .any(|(c, h, l, _)| c == &signature.cb && h == &ha && l == &signature.l)
                        {
                            message
                                .body_hashes
                                .push((signature.cb, ha, signature.l, Vec::new()));
                        }
                    } else {
                        has_arc_errors = true;
                    }

                    message
                        .ams_headers
                        .push(Header::new(name, value, signature));
                    name
                }
                AuthenticatedHeader::As(name) => {
                    let seal = arc::Seal::parse(value);
                    if !has_arc_errors {
                        has_arc_errors = seal.is_err();
                    }
                    message.as_headers.push(Header::new(name, value, seal));
                    name
                }
                AuthenticatedHeader::From(name) => {
                    match MessageStream::new(value).parse_address() {
                        HeaderValue::Address(addr) => {
                            if let Some(addr) = addr.address {
                                message.from.push(addr.to_lowercase());
                            }
                        }
                        HeaderValue::AddressList(list) => {
                            message.from.extend(
                                list.into_iter()
                                    .filter_map(|a| a.address.map(|a| a.to_lowercase())),
                            );
                        }
                        HeaderValue::Group(group) => {
                            message.from.extend(
                                group
                                    .addresses
                                    .into_iter()
                                    .filter_map(|a| a.address.map(|a| a.to_lowercase())),
                            );
                        }
                        HeaderValue::GroupList(group_list) => {
                            message
                                .from
                                .extend(group_list.into_iter().flat_map(|group| {
                                    group
                                        .addresses
                                        .into_iter()
                                        .filter_map(|a| a.address.map(|a| a.to_lowercase()))
                                }))
                        }
                        _ => (),
                    }

                    name
                }
                AuthenticatedHeader::Other(name) => name,
            };

            message.headers.push((name, value));
        }

        if message.headers.is_empty() {
            return None;
        }

        // Obtain message body
        message.body = headers
            .body_offset()
            .and_then(|pos| raw_message.get(pos..))
            .unwrap_or_default();

        // Calculate body hashes
        for (cb, ha, l, bh) in &mut message.body_hashes {
            *bh = match ha {
                HashAlgorithm::Sha256 => cb.hash_body::<Sha256>(message.body, *l),
                HashAlgorithm::Sha1 => cb.hash_body::<Sha1>(message.body, *l),
            }
            .unwrap_or_default();
        }

        // Sort ARC headers
        if !message.as_headers.is_empty() && !has_arc_errors {
            message.as_headers.sort_unstable_by(|a, b| {
                a.header
                    .as_ref()
                    .unwrap()
                    .i
                    .cmp(&b.header.as_ref().unwrap().i)
            });
            message.ams_headers.sort_unstable_by(|a, b| {
                a.header
                    .as_ref()
                    .unwrap()
                    .i
                    .cmp(&b.header.as_ref().unwrap().i)
            });
            message.aar_headers.sort_unstable_by(|a, b| {
                a.header
                    .as_ref()
                    .unwrap()
                    .i
                    .cmp(&b.header.as_ref().unwrap().i)
            });
        }

        message.into()
    }
}
