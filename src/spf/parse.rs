use std::{
    net::{Ipv4Addr, Ipv6Addr},
    slice::Iter,
};

use crate::common::parse::{TagParser, V};

use super::{Directive, Error, Macro, Mechanism, Modifier, Qualifier, Variable, SPF};

impl SPF {
    pub fn parse(bytes: &[u8]) -> super::Result<SPF> {
        let mut record = bytes.iter();
        if !matches!(record.key(), Some(k) if k == V) {
            return Err(Error::InvalidRecord);
        } else if !record.match_bytes(b"spf1")
            || record.next().map_or(false, |v| !v.is_ascii_whitespace())
        {
            return Err(Error::InvalidVersion);
        }

        let mut spf = SPF {
            version: super::Version::Spf1,
            directives: Vec::new(),
            modifiers: Vec::new(),
        };

        while let Some((term, qualifier, mut stop_char)) = record.next_term() {
            match term {
                A | MX => {
                    let mut ip4_cidr_length = 32;
                    let mut ip6_cidr_length = 128;
                    let mut macro_string = Macro::None;

                    match stop_char {
                        b' ' => (),
                        b':' | b'=' => {
                            let (ds, stop_char) = record.macro_string(false)?;
                            macro_string = ds;
                            if stop_char == b'/' {
                                let (l1, l2) = record.dual_cidr_length()?;
                                ip4_cidr_length = l1;
                                ip6_cidr_length = l2;
                            } else if stop_char != b' ' {
                                return Err(Error::ParseFailed);
                            }
                        }
                        b'/' => {
                            let (l1, l2) = record.dual_cidr_length()?;
                            ip4_cidr_length = l1;
                            ip6_cidr_length = l2;
                        }
                        _ => return Err(Error::ParseFailed),
                    }

                    spf.directives.push(Directive::new(
                        qualifier,
                        if term == A {
                            Mechanism::A {
                                macro_string,
                                ip4_cidr_length,
                                ip6_cidr_length,
                            }
                        } else {
                            Mechanism::Mx {
                                macro_string,
                                ip4_cidr_length,
                                ip6_cidr_length,
                            }
                        },
                    ));
                }
                ALL => {
                    if stop_char == b' ' {
                        spf.directives
                            .push(Directive::new(qualifier, Mechanism::All))
                    } else {
                        return Err(Error::ParseFailed);
                    }
                }
                INCLUDE | EXISTS => {
                    if stop_char != b':' {
                        return Err(Error::ParseFailed);
                    }
                    let (macro_string, stop_char) = record.macro_string(false)?;
                    if stop_char == b' ' {
                        spf.directives.push(Directive::new(
                            qualifier,
                            if term == INCLUDE {
                                Mechanism::Include { macro_string }
                            } else {
                                Mechanism::Exists { macro_string }
                            },
                        ));
                    } else {
                        return Err(Error::ParseFailed);
                    }
                }
                IP4 => {
                    if stop_char != b':' {
                        return Err(Error::ParseFailed);
                    }
                    let mut cidr_length = 32;
                    let (addr, stop_char) = record.ip4()?;
                    if stop_char == b'/' {
                        cidr_length = std::cmp::min(cidr_length, record.cidr_length()?);
                    } else if stop_char != b' ' {
                        return Err(Error::ParseFailed);
                    }
                    spf.directives.push(Directive::new(
                        qualifier,
                        Mechanism::Ip4 { addr, cidr_length },
                    ));
                }
                IP6 => {
                    if stop_char != b':' {
                        return Err(Error::ParseFailed);
                    }
                    let mut cidr_length = 128;
                    let (addr, stop_char) = record.ip6()?;
                    if stop_char == b'/' {
                        cidr_length = std::cmp::min(cidr_length, record.cidr_length()?);
                    } else if stop_char != b' ' {
                        return Err(Error::ParseFailed);
                    }
                    spf.directives.push(Directive::new(
                        qualifier,
                        Mechanism::Ip6 { addr, cidr_length },
                    ));
                }
                PTR => {
                    let mut macro_string = Macro::None;
                    if stop_char == b':' {
                        let (ds, stop_char_) = record.macro_string(false)?;
                        macro_string = ds;
                        stop_char = stop_char_;
                    }

                    if stop_char == b' ' {
                        spf.directives
                            .push(Directive::new(qualifier, Mechanism::Ptr { macro_string }));
                    } else {
                        return Err(Error::ParseFailed);
                    }
                }
                EXP | REDIRECT => {
                    if stop_char != b'=' {
                        return Err(Error::ParseFailed);
                    }
                    let (macro_string, stop_char) = record.macro_string(false)?;
                    if stop_char != b' ' {
                        return Err(Error::ParseFailed);
                    }
                    spf.modifiers.push(if term == REDIRECT {
                        Modifier::Redirect(macro_string)
                    } else {
                        Modifier::Explanation(macro_string)
                    });
                }
                _ => {
                    let (_, stop_char) = record.macro_string(false)?;
                    if stop_char != b' ' {
                        return Err(Error::ParseFailed);
                    }
                }
            }
        }

        Ok(spf)
    }
}

const A: u64 = b'a' as u64;
const ALL: u64 = (b'l' as u64) << 16 | (b'l' as u64) << 8 | (b'a' as u64);
const EXISTS: u64 = (b's' as u64) << 40
    | (b't' as u64) << 32
    | (b's' as u64) << 24
    | (b'i' as u64) << 16
    | (b'x' as u64) << 8
    | (b'e' as u64);
const EXP: u64 = (b'p' as u64) << 16 | (b'x' as u64) << 8 | (b'e' as u64);
const INCLUDE: u64 = (b'e' as u64) << 48
    | (b'd' as u64) << 40
    | (b'u' as u64) << 32
    | (b'l' as u64) << 24
    | (b'c' as u64) << 16
    | (b'n' as u64) << 8
    | (b'i' as u64);
const IP4: u64 = (b'4' as u64) << 16 | (b'p' as u64) << 8 | (b'i' as u64);
const IP6: u64 = (b'6' as u64) << 16 | (b'p' as u64) << 8 | (b'i' as u64);
const MX: u64 = (b'x' as u64) << 8 | (b'm' as u64);
const PTR: u64 = (b'r' as u64) << 16 | (b't' as u64) << 8 | (b'p' as u64);
const REDIRECT: u64 = (b't' as u64) << 56
    | (b'c' as u64) << 48
    | (b'e' as u64) << 40
    | (b'r' as u64) << 32
    | (b'i' as u64) << 24
    | (b'd' as u64) << 16
    | (b'e' as u64) << 8
    | (b'r' as u64);

pub(crate) trait SPFParser: Sized {
    fn next_term(&mut self) -> Option<(u64, Qualifier, u8)>;
    fn macro_string(&mut self, is_exp: bool) -> super::Result<(Macro, u8)>;
    fn ip4(&mut self) -> super::Result<(Ipv4Addr, u8)>;
    fn ip6(&mut self) -> super::Result<(Ipv6Addr, u8)>;
    fn cidr_length(&mut self) -> super::Result<u8>;
    fn dual_cidr_length(&mut self) -> super::Result<(u8, u8)>;
}

impl SPFParser for Iter<'_, u8> {
    fn next_term(&mut self) -> Option<(u64, Qualifier, u8)> {
        let mut qualifier = Qualifier::Pass;
        let mut stop_char = b' ';
        let mut d = 0;
        let mut shift = 0;

        for &ch in self {
            match ch {
                b'a'..=b'z' | b'4' | b'6' if shift < 64 => {
                    d |= (ch as u64) << shift;
                    shift += 8;
                }
                b'A'..=b'Z' if shift < 64 => {
                    d |= ((ch - b'A' + b'a') as u64) << shift;
                    shift += 8;
                }
                b'+' if shift == 0 => {
                    qualifier = Qualifier::Pass;
                }
                b'-' if shift == 0 => {
                    qualifier = Qualifier::Fail;
                }
                b'~' if shift == 0 => {
                    qualifier = Qualifier::SoftFail;
                }
                b'?' if shift == 0 => {
                    qualifier = Qualifier::Neutral;
                }
                b':' | b'=' | b'/' => {
                    stop_char = ch;
                    break;
                }
                _ => {
                    if ch.is_ascii_whitespace() {
                        if shift != 0 {
                            stop_char = b' ';
                            break;
                        }
                    } else {
                        d = u64::MAX;
                        shift = 64;
                    }
                }
            }
        }

        if d != 0 {
            (d, qualifier, stop_char).into()
        } else {
            None
        }
    }

    #[allow(clippy::while_let_on_iterator)]
    fn macro_string(&mut self, is_exp: bool) -> super::Result<(Macro, u8)> {
        let mut stop_char = b' ';
        let mut last_is_pct = false;
        let mut literal = Vec::with_capacity(16);
        let mut macro_string = Vec::new();

        while let Some(&ch) = self.next() {
            match ch {
                b'%' => {
                    if last_is_pct {
                        literal.push(b'%');
                    } else {
                        last_is_pct = true;
                        continue;
                    }
                }
                b'_' if last_is_pct => {
                    literal.push(b' ');
                }
                b'-' if last_is_pct => {
                    literal.extend_from_slice(b"%20");
                }
                b'{' if last_is_pct => {
                    if !literal.is_empty() {
                        macro_string.push(Macro::Literal(literal.to_vec()));
                        literal.clear();
                    }

                    let (letter, escape) = self
                        .next()
                        .copied()
                        .and_then(|l| {
                            if !is_exp {
                                Variable::parse(l)
                            } else {
                                Variable::parse_exp(l)
                            }
                        })
                        .ok_or(Error::InvalidMacro)?;
                    let mut num_parts: u32 = 0;
                    let mut reverse = false;
                    let mut delimiters = 0;

                    while let Some(&ch) = self.next() {
                        match ch {
                            b'0'..=b'9' => {
                                num_parts = num_parts
                                    .saturating_mul(10)
                                    .saturating_add((ch - b'0') as u32);
                            }
                            b'r' | b'R' => {
                                reverse = true;
                            }
                            b'}' => {
                                break;
                            }
                            b'.' | b'-' | b'+' | b',' | b'/' | b'_' | b'=' => {
                                delimiters |= 1u64 << (ch - b'+');
                            }
                            _ => {
                                return Err(Error::InvalidMacro);
                            }
                        }
                    }

                    if delimiters == 0 {
                        delimiters = 1u64 << (b'.' - b'+');
                    }

                    macro_string.push(Macro::Variable {
                        letter,
                        num_parts,
                        reverse,
                        escape,
                        delimiters,
                    });
                }
                b'/' if !is_exp => {
                    stop_char = ch;
                    break;
                }
                _ => {
                    if last_is_pct {
                        return Err(Error::InvalidMacro);
                    } else if !ch.is_ascii_whitespace() || is_exp {
                        literal.push(ch);
                    } else {
                        break;
                    }
                }
            }

            last_is_pct = false;
        }

        if !literal.is_empty() {
            macro_string.push(Macro::Literal(literal));
        }

        match macro_string.len() {
            1 => Ok((macro_string.pop().unwrap(), stop_char)),
            0 => Err(Error::ParseFailed),
            _ => Ok((Macro::List(macro_string), stop_char)),
        }
    }

    fn ip4(&mut self) -> super::Result<(Ipv4Addr, u8)> {
        let mut stop_char = b' ';
        let mut pos = 0;
        let mut ip = [0u8; 4];

        for &ch in self {
            match ch {
                b'0'..=b'9' => {
                    ip[pos] = (ip[pos].saturating_mul(10)).saturating_add(ch - b'0');
                }
                b'.' if pos < 3 => {
                    pos += 1;
                }
                _ => {
                    stop_char = if ch.is_ascii_whitespace() { b' ' } else { ch };
                    break;
                }
            }
        }

        if pos == 3 {
            Ok((Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]), stop_char))
        } else {
            Err(Error::InvalidIp4)
        }
    }

    fn ip6(&mut self) -> super::Result<(Ipv6Addr, u8)> {
        let mut stop_char = b' ';
        let mut ip = [0u16; 8];
        let mut ip_pos = 0;
        let mut ip4_pos = 0;
        let mut ip_part = [0u8; 8];
        let mut ip_part_pos = 0;
        let mut zero_group_pos = usize::MAX;

        for &ch in self {
            match ch {
                b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                    if ip_part_pos < 4 {
                        ip_part[ip_part_pos] = ch;
                        ip_part_pos += 1;
                    } else {
                        return Err(Error::InvalidIp6);
                    }
                }
                b':' => {
                    if ip_pos < 8 {
                        if ip_part_pos != 0 {
                            ip[ip_pos] = u16::from_str_radix(
                                std::str::from_utf8(&ip_part[..ip_part_pos]).unwrap(),
                                16,
                            )
                            .map_err(|_| Error::InvalidIp6)?;
                            ip_part_pos = 0;
                            ip_pos += 1;
                        } else if zero_group_pos == usize::MAX {
                            zero_group_pos = ip_pos;
                        } else if zero_group_pos != ip_pos {
                            return Err(Error::InvalidIp6);
                        }
                    } else {
                        return Err(Error::InvalidIp6);
                    }
                }
                b'.' => {
                    if ip_pos < 8 && ip_part_pos > 0 {
                        let qnum = std::str::from_utf8(&ip_part[..ip_part_pos])
                            .unwrap()
                            .parse::<u8>()
                            .map_err(|_| Error::InvalidIp6)?
                            as u16;
                        ip_part_pos = 0;
                        if ip4_pos % 2 == 1 {
                            ip[ip_pos] = (ip[ip_pos] << 8) | qnum;
                            ip_pos += 1;
                        } else {
                            ip[ip_pos] = qnum;
                        }
                        ip4_pos += 1;
                    } else {
                        return Err(Error::InvalidIp6);
                    }
                }
                _ => {
                    stop_char = if ch.is_ascii_whitespace() { b' ' } else { ch };
                    break;
                }
            }
        }

        if ip_part_pos != 0 {
            if ip_pos < 8 {
                ip[ip_pos] = if ip4_pos == 0 {
                    u16::from_str_radix(std::str::from_utf8(&ip_part[..ip_part_pos]).unwrap(), 16)
                        .map_err(|_| Error::InvalidIp6)?
                } else if ip4_pos == 3 {
                    (ip[ip_pos] << 8)
                        | std::str::from_utf8(&ip_part[..ip_part_pos])
                            .unwrap()
                            .parse::<u8>()
                            .map_err(|_| Error::InvalidIp6)? as u16
                } else {
                    return Err(Error::InvalidIp6);
                };

                ip_pos += 1;
            } else {
                return Err(Error::InvalidIp6);
            }
        }
        if zero_group_pos != usize::MAX && zero_group_pos < ip_pos {
            if ip_pos < 7 {
                ip.copy_within(zero_group_pos..ip_pos, zero_group_pos + 8 - ip_pos);
                ip[zero_group_pos..zero_group_pos + 8 - ip_pos].fill(0);
            } else {
                return Err(Error::InvalidIp6);
            }
        }

        if ip_pos != 0 || zero_group_pos != usize::MAX {
            Ok((
                Ipv6Addr::new(ip[0], ip[1], ip[2], ip[3], ip[4], ip[5], ip[6], ip[7]),
                stop_char,
            ))
        } else {
            Err(Error::InvalidIp6)
        }
    }

    fn cidr_length(&mut self) -> super::Result<u8> {
        let mut cidr_length: u8 = 0;
        for &ch in self {
            match ch {
                b'0'..=b'9' => {
                    cidr_length = (cidr_length.saturating_mul(10)).saturating_add(ch - b'0');
                }
                _ => {
                    if ch.is_ascii_whitespace() {
                        break;
                    } else {
                        return Err(Error::ParseFailed);
                    }
                }
            }
        }

        Ok(cidr_length)
    }

    fn dual_cidr_length(&mut self) -> super::Result<(u8, u8)> {
        let mut ip4_length: u8 = u8::MAX;
        let mut ip6_length: u8 = u8::MAX;
        let mut in_ip6 = false;

        for &ch in self {
            match ch {
                b'0'..=b'9' => {
                    if in_ip6 {
                        ip6_length = if ip6_length != u8::MAX {
                            (ip6_length.saturating_mul(10)).saturating_add(ch - b'0')
                        } else {
                            ch - b'0'
                        };
                    } else {
                        ip4_length = if ip4_length != u8::MAX {
                            (ip4_length.saturating_mul(10)).saturating_add(ch - b'0')
                        } else {
                            ch - b'0'
                        };
                    }
                }
                b'/' => {
                    if !in_ip6 {
                        in_ip6 = true;
                    } else if ip6_length != u8::MAX {
                        return Err(Error::ParseFailed);
                    }
                }
                _ => {
                    if ch.is_ascii_whitespace() {
                        break;
                    } else {
                        return Err(Error::ParseFailed);
                    }
                }
            }
        }

        Ok((
            std::cmp::min(ip4_length, 32),
            std::cmp::min(ip6_length, 128),
        ))
    }
}

impl Variable {
    fn parse(ch: u8) -> Option<(Self, bool)> {
        match ch {
            b's' => (Variable::Sender, false),
            b'l' => (Variable::SenderLocalPart, false),
            b'o' => (Variable::SenderDomainPart, false),
            b'd' => (Variable::Domain, false),
            b'i' => (Variable::Ip, false),
            b'p' => (Variable::ValidatedDomain, false),
            b'v' => (Variable::IpVersion, false),
            b'h' => (Variable::HeloDomain, false),

            b'S' => (Variable::Sender, true),
            b'L' => (Variable::SenderLocalPart, true),
            b'O' => (Variable::SenderDomainPart, true),
            b'D' => (Variable::Domain, true),
            b'I' => (Variable::Ip, true),
            b'P' => (Variable::ValidatedDomain, true),
            b'V' => (Variable::IpVersion, true),
            b'H' => (Variable::HeloDomain, true),
            _ => return None,
        }
        .into()
    }

    fn parse_exp(ch: u8) -> Option<(Self, bool)> {
        match ch {
            b's' => (Variable::Sender, false),
            b'l' => (Variable::SenderLocalPart, false),
            b'o' => (Variable::SenderDomainPart, false),
            b'd' => (Variable::Domain, false),
            b'i' => (Variable::Ip, false),
            b'p' => (Variable::ValidatedDomain, false),
            b'v' => (Variable::IpVersion, false),
            b'h' => (Variable::HeloDomain, false),
            b'c' => (Variable::SmtpIp, false),
            b'r' => (Variable::HostDomain, false),
            b't' => (Variable::CurrentTime, false),

            b'S' => (Variable::Sender, true),
            b'L' => (Variable::SenderLocalPart, true),
            b'O' => (Variable::SenderDomainPart, true),
            b'D' => (Variable::Domain, true),
            b'I' => (Variable::Ip, true),
            b'P' => (Variable::ValidatedDomain, true),
            b'V' => (Variable::IpVersion, true),
            b'H' => (Variable::HeloDomain, true),
            b'C' => (Variable::SmtpIp, true),
            b'R' => (Variable::HostDomain, true),
            b'T' => (Variable::CurrentTime, true),
            _ => return None,
        }
        .into()
    }
}

#[cfg(test)]
mod test {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use crate::spf::{Directive, Macro, Mechanism, Modifier, Qualifier, Variable, Version, SPF};

    use super::SPFParser;

    #[test]
    fn parse_spf() {
        for (record, expected_result) in [
            (
                "v=spf1 +mx a:colo.example.com/28 -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::A {
                                macro_string: Macro::Literal(b"colo.example.com".to_vec()),
                                ip4_cidr_length: 28,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 a:A.EXAMPLE.COM -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::A {
                                macro_string: Macro::Literal(b"A.EXAMPLE.COM".to_vec()),
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 +mx -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 +mx redirect=_spf.example.com",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![Modifier::Redirect(Macro::Literal(
                        b"_spf.example.com".to_vec(),
                    ))],
                    directives: vec![Directive::new(
                        Qualifier::Pass,
                        Mechanism::Mx {
                            macro_string: Macro::None,
                            ip4_cidr_length: 32,
                            ip6_cidr_length: 128,
                        },
                    )],
                },
            ),
            (
                "v=spf1 a mx -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::A {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 include:example.com include:example.org -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Include {
                                macro_string: Macro::Literal(b"example.com".to_vec()),
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Include {
                                macro_string: Macro::Literal(b"example.org".to_vec()),
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 exists:%{ir}.%{l1r+-}._spf.%{d} -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Exists {
                                macro_string: Macro::List(vec![
                                    Macro::Variable {
                                        letter: Variable::Ip,
                                        num_parts: 0,
                                        reverse: true,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b".".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::SenderLocalPart,
                                        num_parts: 1,
                                        reverse: true,
                                        escape: false,
                                        delimiters: 1u64 << (b'+' - b'+') | 1u64 << (b'-' - b'+'),
                                    },
                                    Macro::Literal(b"._spf.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::Domain,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                ]),
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 mx -all exp=explain._spf.%{d}",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![Modifier::Explanation(Macro::List(vec![
                        Macro::Literal(b"explain._spf.".to_vec()),
                        Macro::Variable {
                            letter: Variable::Domain,
                            num_parts: 0,
                            reverse: false,
                            escape: false,
                            delimiters: 1u64 << (b'.' - b'+'),
                        },
                    ]))],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 ip4:192.0.2.1 ip4:192.0.2.129 -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Ip4 {
                                addr: "192.0.2.1".parse().unwrap(),
                                cidr_length: 32,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Ip4 {
                                addr: "192.0.2.129".parse().unwrap(),
                                cidr_length: 32,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 ip4:192.0.2.0/24 mx -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Ip4 {
                                addr: "192.0.2.0".parse().unwrap(),
                                cidr_length: 24,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 mx/30 mx:example.org/30 -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 30,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::Literal(b"example.org".to_vec()),
                                ip4_cidr_length: 30,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 ptr -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Ptr {
                                macro_string: Macro::None,
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 exists:%{l1r+}.%{d}",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![Directive::new(
                        Qualifier::Pass,
                        Mechanism::Exists {
                            macro_string: Macro::List(vec![
                                Macro::Variable {
                                    letter: Variable::SenderLocalPart,
                                    num_parts: 1,
                                    reverse: true,
                                    escape: false,
                                    delimiters: 1u64 << (b'+' - b'+'),
                                },
                                Macro::Literal(b".".to_vec()),
                                Macro::Variable {
                                    letter: Variable::Domain,
                                    num_parts: 0,
                                    reverse: false,
                                    escape: false,
                                    delimiters: 1u64 << (b'.' - b'+'),
                                },
                            ]),
                        },
                    )],
                },
            ),
            (
                "v=spf1 exists:%{ir}.%{l1r+}.%{d}",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![Directive::new(
                        Qualifier::Pass,
                        Mechanism::Exists {
                            macro_string: Macro::List(vec![
                                Macro::Variable {
                                    letter: Variable::Ip,
                                    num_parts: 0,
                                    reverse: true,
                                    escape: false,
                                    delimiters: 1u64 << (b'.' - b'+'),
                                },
                                Macro::Literal(b".".to_vec()),
                                Macro::Variable {
                                    letter: Variable::SenderLocalPart,
                                    num_parts: 1,
                                    reverse: true,
                                    escape: false,
                                    delimiters: 1u64 << (b'+' - b'+'),
                                },
                                Macro::Literal(b".".to_vec()),
                                Macro::Variable {
                                    letter: Variable::Domain,
                                    num_parts: 0,
                                    reverse: false,
                                    escape: false,
                                    delimiters: 1u64 << (b'.' - b'+'),
                                },
                            ]),
                        },
                    )],
                },
            ),
            (
                "v=spf1 exists:_h.%{h}._l.%{l}._o.%{o}._i.%{i}._spf.%{d} ?all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Exists {
                                macro_string: Macro::List(vec![
                                    Macro::Literal(b"_h.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::HeloDomain,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b"._l.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::SenderLocalPart,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b"._o.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::SenderDomainPart,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b"._i.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::Ip,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b"._spf.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::Domain,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                ]),
                            },
                        ),
                        Directive::new(Qualifier::Neutral, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 mx ?exists:%{ir}.whitelist.example.org -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(
                            Qualifier::Neutral,
                            Mechanism::Exists {
                                macro_string: Macro::List(vec![
                                    Macro::Variable {
                                        letter: Variable::Ip,
                                        num_parts: 0,
                                        reverse: true,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b".whitelist.example.org".to_vec()),
                                ]),
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 mx exists:%{l}._%-spf_%_verify%%.%{d} -all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 128,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Exists {
                                macro_string: Macro::List(vec![
                                    Macro::Variable {
                                        letter: Variable::SenderLocalPart,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                    Macro::Literal(b"._%20spf_ verify%.".to_vec()),
                                    Macro::Variable {
                                        letter: Variable::Domain,
                                        num_parts: 0,
                                        reverse: false,
                                        escape: false,
                                        delimiters: 1u64 << (b'.' - b'+'),
                                    },
                                ]),
                            },
                        ),
                        Directive::new(Qualifier::Fail, Mechanism::All),
                    ],
                },
            ),
            (
                "v=spf1 mx redirect=%{l1r+}._at_.%{o,=_/}._spf.%{d}",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![Modifier::Redirect(Macro::List(vec![
                        Macro::Variable {
                            letter: Variable::SenderLocalPart,
                            num_parts: 1,
                            reverse: true,
                            escape: false,
                            delimiters: 1u64 << (b'+' - b'+'),
                        },
                        Macro::Literal(b"._at_.".to_vec()),
                        Macro::Variable {
                            letter: Variable::SenderDomainPart,
                            num_parts: 0,
                            reverse: false,
                            escape: false,
                            delimiters: 1u64 << (b',' - b'+')
                                | 1u64 << (b'=' - b'+')
                                | 1u64 << (b'_' - b'+')
                                | 1u64 << (b'/' - b'+'),
                        },
                        Macro::Literal(b"._spf.".to_vec()),
                        Macro::Variable {
                            letter: Variable::Domain,
                            num_parts: 0,
                            reverse: false,
                            escape: false,
                            delimiters: 1u64 << (b'.' - b'+'),
                        },
                    ]))],
                    directives: vec![Directive::new(
                        Qualifier::Pass,
                        Mechanism::Mx {
                            macro_string: Macro::None,
                            ip4_cidr_length: 32,
                            ip6_cidr_length: 128,
                        },
                    )],
                },
            ),
            (
                "v=spf1 -ip4:192.0.2.0/24 a//96 +all",
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Fail,
                            Mechanism::Ip4 {
                                addr: "192.0.2.0".parse().unwrap(),
                                cidr_length: 24,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::A {
                                macro_string: Macro::None,
                                ip4_cidr_length: 32,
                                ip6_cidr_length: 96,
                            },
                        ),
                        Directive::new(Qualifier::Pass, Mechanism::All),
                    ],
                },
            ),
            (
                concat!(
                    "v=spf1 +mx/11//100 ~a:domain.com/12/123 ?ip6:::1 ",
                    "-ip6:a::b/111 ip6:1080::8:800:68.0.3.1/96 "
                ),
                SPF {
                    version: Version::Spf1,
                    modifiers: vec![],
                    directives: vec![
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Mx {
                                macro_string: Macro::None,
                                ip4_cidr_length: 11,
                                ip6_cidr_length: 100,
                            },
                        ),
                        Directive::new(
                            Qualifier::SoftFail,
                            Mechanism::A {
                                macro_string: Macro::Literal(b"domain.com".to_vec()),
                                ip4_cidr_length: 12,
                                ip6_cidr_length: 123,
                            },
                        ),
                        Directive::new(
                            Qualifier::Neutral,
                            Mechanism::Ip6 {
                                addr: "::1".parse().unwrap(),
                                cidr_length: 128,
                            },
                        ),
                        Directive::new(
                            Qualifier::Fail,
                            Mechanism::Ip6 {
                                addr: "a::b".parse().unwrap(),
                                cidr_length: 111,
                            },
                        ),
                        Directive::new(
                            Qualifier::Pass,
                            Mechanism::Ip6 {
                                addr: "1080::8:800:68.0.3.1".parse().unwrap(),
                                cidr_length: 96,
                            },
                        ),
                    ],
                },
            ),
        ] {
            assert_eq!(
                SPF::parse(record.as_bytes())
                    .unwrap_or_else(|err| panic!("{:?} : {:?}", record, err)),
                expected_result,
                "{}",
                record
            );
        }
    }

    #[test]
    fn parse_ip6() {
        for test in [
            "ABCD:EF01:2345:6789:ABCD:EF01:2345:6789",
            "2001:DB8:0:0:8:800:200C:417A",
            "FF01:0:0:0:0:0:0:101",
            "0:0:0:0:0:0:0:1",
            "0:0:0:0:0:0:0:0",
            "2001:DB8::8:800:200C:417A",
            "2001:DB8:0:0:8:800:200C::",
            "FF01::101",
            "::1",
            "::",
            "a:b::c:d",
            "a::c:d",
            "a:b:c::d",
            "::c:d",
            "0:0:0:0:0:0:13.1.68.3",
            "0:0:0:0:0:FFFF:129.144.52.38",
            "::13.1.68.3",
            "::FFFF:129.144.52.38",
        ] {
            for test in [test.to_string(), format!("{} ", test)] {
                let (ip, stop_char) = test
                    .as_bytes()
                    .iter()
                    .ip6()
                    .unwrap_or_else(|err| panic!("{:?} : {:?}", test, err));
                assert_eq!(stop_char, b' ', "{}", test);
                assert_eq!(ip, test.trim_end().parse::<Ipv6Addr>().unwrap())
            }
        }

        for invalid_test in [
            "0:0:0:0:0:0:0:1:1",
            "0:0:0:0:0:0:13.1.68.3.4",
            "::0:0:0:0:0:0:0:0",
            "0:0:0:0::0:0:0:0",
            " ",
            "",
        ] {
            assert!(
                invalid_test.as_bytes().iter().ip6().is_err(),
                "{}",
                invalid_test
            );
        }
    }

    #[test]
    fn parse_ip4() {
        for test in ["0.0.0.0", "255.255.255.255", "13.1.68.3", "129.144.52.38"] {
            for test in [test.to_string(), format!("{} ", test)] {
                let (ip, stop_char) = test
                    .as_bytes()
                    .iter()
                    .ip4()
                    .unwrap_or_else(|err| panic!("{:?} : {:?}", test, err));
                assert_eq!(stop_char, b' ', "{}", test);
                assert_eq!(ip, test.trim_end().parse::<Ipv4Addr>().unwrap());
            }
        }
    }
}
