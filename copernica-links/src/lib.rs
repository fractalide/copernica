mod udp;
mod mpsc_channel;
mod mpsc_corruptor;
pub use {
    udp::{UdpIp},
    mpsc_channel::{MpscChannel},
    mpsc_corruptor::{MpscCorruptor},
};
use {
    copernica_common::{
        InterLinkPacket, LinkId,
        constants::*,
        HBFI, ReplyTo,
        NarrowWaistPacket, ResponseData, LinkPacket, BFI,
        generate_nonce, Nonce, Tag, Data
    },
    cryptoxide::{chacha20poly1305::{ChaCha20Poly1305}},
    copernica_identity::{ PrivateIdentity, PublicIdentity, Signature },
    bincode,
    log::{trace},
    crossbeam_channel::{Sender, Receiver},
    anyhow::{anyhow, Result},
    reed_solomon::{Buffer, Encoder, Decoder},
};
fn u16_to_u8(i: u16) -> [u8; 2] {
    [(i >> 8) as u8, i as u8]
}
fn u8_to_u16(i: [u8; 2]) -> u16 {
    ((i[0] as u16) << 8) | i[1] as u16
}
fn bfi_to_u8(bfi: BFI) -> [u8; BFI_BYTE_SIZE] {
    let mut bbfi: [u8; BFI_BYTE_SIZE] = [0; BFI_BYTE_SIZE];
    let mut count = 0;
    for i in bfi.iter() {
        let two_u8 = u16_to_u8(*i);
        bbfi[count]   = two_u8[0];
        bbfi[count+1] = two_u8[1];
        count+=2;
    }
    bbfi
}
fn u8_to_bfi(bbfi: [u8; BFI_BYTE_SIZE]) -> BFI {
    [((bbfi[0] as u16) << 8) | bbfi[1] as u16,
    ((bbfi[2]  as u16) << 8) | bbfi[3] as u16,
    ((bbfi[4]  as u16) << 8) | bbfi[5] as u16,
    ((bbfi[6]  as u16) << 8) | bbfi[7] as u16]
}
fn u8_to_u64(v: [u8; 8]) -> u64 {
    let mut x: u64 = 0;
    for i in 0..v.len() {
        x = ((x << 8) | v[i] as u64) as u64;
    }
    x
}
fn u64_to_u8(x: u64) -> [u8; 8] {
    [((x >> 56) & 0xff) as u8,
    ((x  >> 48) & 0xff) as u8,
    ((x  >> 40) & 0xff) as u8,
    ((x  >> 32) & 0xff) as u8,
    ((x  >> 24) & 0xff) as u8,
    ((x  >> 16) & 0xff) as u8,
    ((x  >> 8)  & 0xff) as u8,
    (x          & 0xff) as u8]
}
pub fn serialize_response_data(rd: &ResponseData) -> (u16, Vec<u8>) {
    let mut buf: Vec<u8> = vec![];
    match rd {
        ResponseData::ClearText { data } => {
            buf.extend_from_slice(&data.raw_data());
            (buf.len() as u16, buf)
        },
        ResponseData::CypherText { data, tag } => {
            buf.extend_from_slice(tag.as_ref());
            buf.extend_from_slice(&data.raw_data());
            (buf.len() as u16, buf)
        },
    }
}
pub fn deserialize_cyphertext_response_data(data: &Vec<u8>) -> Result<ResponseData> {
    let mut tag = [0u8; TAG_SIZE];
    tag.clone_from_slice(&data[..TAG_SIZE]);
    let data = Data::new(data[TAG_SIZE..].to_vec())?;
    Ok(ResponseData::reconstitute_cypher_text(tag, data))
}
pub fn deserialize_cleartext_response_data(data: &Vec<u8>) -> Result<ResponseData> {
    let data = Data::new(data[..].to_vec())?;
    Ok(ResponseData::reconstitute_clear_text(data))
}
fn serialize_hbfi(hbfi: &HBFI) -> Result<(u8, Vec<u8>)> {
    let mut buf: Vec<u8> = vec![];
    let res = &bfi_to_u8(hbfi.res);
    let req = &bfi_to_u8(hbfi.req);
    let app = &bfi_to_u8(hbfi.app);
    let m0d = &bfi_to_u8(hbfi.m0d);
    let fun = &bfi_to_u8(hbfi.fun);
    let arg = &bfi_to_u8(hbfi.arg);
    let ost = &u64_to_u8(hbfi.ost);
    let mut ids_buf: Vec<u8> = vec![];
    match &hbfi.request_pid {
        Some(request_pid) => {
            ids_buf.extend_from_slice(hbfi.response_pid.key().as_ref());
            ids_buf.extend_from_slice(hbfi.response_pid.chain_code().as_ref());
            ids_buf.extend_from_slice(request_pid.key().as_ref());
            ids_buf.extend_from_slice(request_pid.chain_code().as_ref());
        },
        None => {
            ids_buf.extend_from_slice(hbfi.response_pid.key().as_ref());
            ids_buf.extend_from_slice(hbfi.response_pid.chain_code().as_ref());
        },
    }
    buf.extend_from_slice(res);
    trace!("ser \thbfi 0: \t\t{:?}", res.as_ref());
    buf.extend_from_slice(req);
    trace!("ser \thbfi 1: \t\t{:?}", req.as_ref());
    buf.extend_from_slice(app);
    trace!("ser \thbfi 2: \t\t{:?}", app.as_ref());
    buf.extend_from_slice(m0d);
    trace!("ser \thbfi 3: \t\t{:?}", m0d.as_ref());
    buf.extend_from_slice(fun);
    trace!("ser \thbfi 4: \t\t{:?}", fun.as_ref());
    buf.extend_from_slice(arg);
    trace!("ser \thbfi 5: \t\t{:?}", arg.as_ref());
    buf.extend_from_slice(ost);
    trace!("ser \toffset: \t\t{:?}", ost.as_ref());
    buf.extend_from_slice(&ids_buf);
    trace!("ser \tids: \t\t\t{:?}", ids_buf);
    let size = res.len() + req.len() + app.len() + m0d.len() + fun.len() + arg.len() + ost.len() + ids_buf.len();
    Ok((size as u8, buf))
}
pub fn deserialize_cyphertext_hbfi(data: &Vec<u8>) -> Result<HBFI> {
    let mut bfis: Vec<BFI> = Vec::with_capacity(BFI_COUNT);
    let mut count = 0;
    for _ in 0..BFI_COUNT {
        let mut bbfi = [0u8; BFI_BYTE_SIZE];
        bbfi.clone_from_slice(&data[count..count+BFI_BYTE_SIZE]);
        trace!("des \thbfi {}: \t\t{:?}", count, bbfi.as_ref());
        bfis.push(u8_to_bfi(bbfi));
        count += BFI_BYTE_SIZE;
    }
    let mut ost = [0u8; U64_SIZE];
    ost.clone_from_slice(&data[HBFI_OFFSET_START..HBFI_OFFSET_END]);
    trace!("des \toffset: \t\t{:?}", ost.as_ref());
    let ost: u64 = u8_to_u64(ost);
    let mut res_key = [0u8; ID_SIZE];
    res_key.clone_from_slice(&data[HBFI_RESPONSE_KEY_START..HBFI_RESPONSE_KEY_END]);
    trace!("des \tres_key: \t\t{:?}", res_key);
    let mut res_ccd = [0u8; CC_SIZE];
    res_ccd.clone_from_slice(&data[HBFI_RESPONSE_CHAIN_CODE_START..HBFI_RESPONSE_CHAIN_CODE_END]);
    trace!("des \tres_ccd: \t\t{:?}", res_ccd);
    let mut req_key = [0u8; ID_SIZE];
    req_key.clone_from_slice(&data[HBFI_REQUEST_KEY_START..HBFI_REQUEST_KEY_END]);
    trace!("des \treq_key: \t\t{:?}", req_key);
    let mut req_ccd = [0u8; CC_SIZE];
    req_ccd.clone_from_slice(&data[HBFI_REQUEST_CHAIN_CODE_START..HBFI_REQUEST_CHAIN_CODE_END]);
    trace!("des \treq_ccd: \t\t{:?}", req_ccd);
    Ok(HBFI { response_pid: PublicIdentity::reconstitute(res_key, res_ccd)
            , request_pid: Some(PublicIdentity::reconstitute(req_key, req_ccd))
            , res: bfis[0], req: bfis[1], app: bfis[2], m0d: bfis[3], fun: bfis[4], arg: bfis[5]
            , ost})
}
pub fn deserialize_cleartext_hbfi(data: &Vec<u8>) -> Result<HBFI> {
    let mut bfis: Vec<BFI> = Vec::with_capacity(BFI_COUNT);
    let mut count = 0;
    for _ in 0..BFI_COUNT {
        let mut bbfi = [0u8; BFI_BYTE_SIZE];
        bbfi.clone_from_slice(&data[count..count+BFI_BYTE_SIZE]);
        trace!("des \thbfi {}: \t\t{:?}", count, bbfi.as_ref());
        bfis.push(u8_to_bfi(bbfi));
        count += BFI_BYTE_SIZE;
    }
    let mut ost = [0u8; U64_SIZE];
    ost.clone_from_slice(&data[HBFI_OFFSET_START..HBFI_OFFSET_END]);
    trace!("des \toffset: \t\t{:?}", ost.as_ref());
    let ost: u64 = u8_to_u64(ost);
    let mut res_key = [0u8; ID_SIZE];
    res_key.clone_from_slice(&data[HBFI_RESPONSE_KEY_START..HBFI_RESPONSE_KEY_END]);
    trace!("des \tres_key: \t\t{:?}", res_key);
    let mut res_ccd = [0u8; CC_SIZE];
    res_ccd.clone_from_slice(&data[HBFI_RESPONSE_CHAIN_CODE_START..HBFI_RESPONSE_CHAIN_CODE_END]);
    trace!("des \tres_ccd: \t\t{:?}", res_ccd);
    Ok(HBFI { response_pid: PublicIdentity::reconstitute(res_key, res_ccd)
            , request_pid: None
            , res: bfis[0], req: bfis[1], app: bfis[2], m0d: bfis[3], fun: bfis[4], arg: bfis[5]
            , ost})
}
pub fn deserialize_cyphertext_narrow_waist_packet_response(data: &Vec<u8>) -> Result<NarrowWaistPacket> {
    let mut signature = [0u8; Signature::SIZE];
    signature.clone_from_slice(&data[CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_SIG_START..CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_SIG_END]);
    trace!("des \tsignature: \t\t{:?}", signature.as_ref());
    let signature: Signature = Signature::reconstitute(&signature);
    let mut offset = [0u8; U64_SIZE];
    offset.clone_from_slice(&data[CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_OFFSET_START..CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_OFFSET_END]);
    trace!("des \toffset: \t\t{:?}", offset.as_ref());
    let offset: u64 = u8_to_u64(offset);
    let mut total = [0u8; U64_SIZE];
    total.clone_from_slice(&data[CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_TOTAL_START..CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_TOTAL_END]);
    trace!("des \ttotal: \t\t\t{:?}", total.as_ref());
    let total: u64 = u8_to_u64(total);
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.clone_from_slice(&data[CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_START..CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_END]);
    trace!("des \tnonce: \t\t\t{:?}", nonce.as_ref());
    let hbfi_end = CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_END + CYPHERTEXT_HBFI_SIZE;
    let response_data_end = hbfi_end + CYPHERTEXT_RESPONSE_DATA_SIZE;
    let hbfi: HBFI = deserialize_cyphertext_hbfi(&data[CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_END..hbfi_end].to_vec())?;
    let data: ResponseData = deserialize_cyphertext_response_data(&data[hbfi_end..response_data_end].to_vec())?;
    let nw: NarrowWaistPacket = NarrowWaistPacket::Response { hbfi, signature, offset, total, nonce, data };
    Ok(nw)
}
pub fn deserialize_cleartext_narrow_waist_packet_response(data: &Vec<u8>) -> Result<NarrowWaistPacket> {
    let mut signature = [0u8; Signature::SIZE];
    signature.clone_from_slice(&data[CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_SIG_START..CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_SIG_END]);
    trace!("des \tsignature: \t\t{:?}", signature.as_ref());
    let signature: Signature = Signature::reconstitute(&signature);
    let mut offset = [0u8; U64_SIZE];
    offset.clone_from_slice(&data[CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_OFFSET_START..CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_OFFSET_END]);
    trace!("des \toffset: \t\t{:?}", offset.as_ref());
    let offset: u64 = u8_to_u64(offset);
    let mut total = [0u8; U64_SIZE];
    total.clone_from_slice(&data[CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_TOTAL_START..CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_TOTAL_END]);
    trace!("des \ttotal: \t\t\t{:?}", total.as_ref());
    let total: u64 = u8_to_u64(total);
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.clone_from_slice(&data[CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_START..CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_END]);
    trace!("des \tnonce: \t\t\t{:?}", nonce.as_ref());
    let hbfi_end = CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_END + CLEARTEXT_HBFI_SIZE;
    let response_data_end = hbfi_end + CLEARTEXT_RESPONSE_DATA_SIZE;
    let hbfi: HBFI = deserialize_cleartext_hbfi(&data[CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_NONCE_END..hbfi_end].to_vec())?;
    let data: ResponseData = deserialize_cleartext_response_data(&data[hbfi_end..response_data_end].to_vec())?;
    let nw: NarrowWaistPacket = NarrowWaistPacket::Response { hbfi, signature, offset, total, nonce, data };
    Ok(nw)
}
pub fn deserialize_cyphertext_narrow_waist_packet_request(data: &Vec<u8>) -> Result<NarrowWaistPacket> {
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.clone_from_slice(&data[0..NONCE_SIZE]);
    let hbfi: HBFI = deserialize_cyphertext_hbfi(&data[NONCE_SIZE..NONCE_SIZE+CYPHERTEXT_HBFI_SIZE].to_vec())?;
    let nw: NarrowWaistPacket = NarrowWaistPacket::Request { hbfi, nonce };
    Ok(nw)
}
pub fn deserialize_cleartext_narrow_waist_packet_request(data: &Vec<u8>) -> Result<NarrowWaistPacket> {
    let mut nonce = [0u8; NONCE_SIZE];
    nonce.clone_from_slice(&data[0..NONCE_SIZE]);
    let hbfi: HBFI = deserialize_cleartext_hbfi(&data[NONCE_SIZE..NONCE_SIZE+CLEARTEXT_HBFI_SIZE].to_vec())?;
    let nw: NarrowWaistPacket = NarrowWaistPacket::Request { hbfi, nonce };
    Ok(nw)
}
pub fn serialize_narrow_waist_packet(nw: &NarrowWaistPacket) -> Result<(u16, Vec<u8>)> {
    let mut buf: Vec<u8> = vec![];
    let size: u16;
    match nw {
        NarrowWaistPacket::Request { hbfi, nonce } => {
            let (hbfi_size, hbfi) = serialize_hbfi(&hbfi)?;
            size = hbfi_size as u16 + nonce.len() as u16;
            buf.extend_from_slice(nonce);
            buf.extend_from_slice(&hbfi);
        },
        NarrowWaistPacket::Response { hbfi, signature, offset, total, nonce, data } => {
            let (hbfi_size, hbfi) = serialize_hbfi(&hbfi)?;
            let (response_data_size, response_data) = serialize_response_data(&data);
            let ost = &u64_to_u8(*offset);
            let tot = &u64_to_u8(*total);
            size = hbfi_size as u16
                + signature.as_ref().len() as u16
                + ost.len() as u16
                + tot.len() as u16
                + nonce.len() as u16
                + response_data_size as u16;
            trace!("ser \tsignature: \t\t{:?}", signature.as_ref());
            buf.extend_from_slice(signature.as_ref());
            trace!("ser \toffset: \t\t{:?}", ost.as_ref());
            buf.extend_from_slice(ost);
            trace!("ser \ttotal: \t\t\t{:?}", tot.as_ref());
            buf.extend_from_slice(tot);
            trace!("ser \tnonce: \t\t\t{:?}", nonce.as_ref());
            buf.extend_from_slice(nonce);
            buf.extend_from_slice(&hbfi);
            buf.extend_from_slice(&response_data);
        },
    }
    Ok((size, buf))
}

fn serialize_reply_to(rt: &ReplyTo) -> Result<(u8, Vec<u8>)> {
    let mut buf: Vec<u8> = vec![];
    let size: u8;
    match rt {
        ReplyTo::Mpsc => {
            size = 0;
            trace!("ser rep_to mpsc: \t\t{:?}", [0]);
        },
        ReplyTo::UdpIp(addr) => {
            let addr_s = bincode::serialize(&addr)?;
            size = addr_s.len() as u8;
            trace!("ser rep_to udpip: \t\t{:?}", addr_s);
            buf.extend_from_slice(addr_s.as_ref());
        }
        ReplyTo::Rf(hz) => {
            let hz = bincode::serialize(&hz)?;
            size = hz.len() as u8;
            trace!("ser rep_to udpip: \t\t{:?}", hz);
            buf.extend_from_slice(hz.as_ref());
        }
    }
    Ok((size, buf))
}

fn deserialize_reply_to(data: &Vec<u8>) -> Result<ReplyTo> {
    let rt = match data.len()as usize {
        TO_REPLY_TO_MPSC => {
            ReplyTo::Mpsc
        },
        TO_REPLY_TO_UDPIP4 => {
            let address = &data[..];
            let address = bincode::deserialize(&address)?;
            ReplyTo::UdpIp(address)
        },
        TO_REPLY_TO_UDPIP6 => {
            let address = &data[..];
            let address = bincode::deserialize(&address)?;
            ReplyTo::UdpIp(address)
        },
        TO_REPLY_TO_RF => {
            let address = &data[..];
            let address = bincode::deserialize(&address)?;
            ReplyTo::Rf(address)
        },
        _ => return Err(anyhow!("Deserializing ReplyTo hit an unrecognised type or variation"))
    };
    Ok(rt)
}

pub fn serialize_link_packet(lp: &LinkPacket, lnk_tx_sid: PrivateIdentity, lnk_rx_pid: Option<PublicIdentity>) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = vec![];
    match lnk_rx_pid {
        None => {
            let reply_to = lp.reply_to();
            let nw = lp.narrow_waist();
            buf.extend_from_slice(lnk_tx_sid.public_id().key().as_ref());
            trace!("ser link_key: \t\t{:?}", lnk_tx_sid.public_id().key().as_ref());
            buf.extend_from_slice(lnk_tx_sid.public_id().chain_code().as_ref());
            trace!("ser link_ccd: \t\t{:?}", lnk_tx_sid.public_id().chain_code().as_ref());
            let (reply_to_size, reply_to) = serialize_reply_to(&reply_to)?;
            trace!("ser reply_to_size: \t\t{:?}", reply_to_size);
            let (nw_size, nw) = serialize_narrow_waist_packet(&nw)?;
            trace!("ser nw_size: \t\t\t{:?}", nw_size);
            buf.extend_from_slice(&[reply_to_size]);
            buf.extend_from_slice(&u16_to_u8(nw_size));
            trace!("ser reply_to: \t\t\t{:?}", reply_to);
            buf.extend_from_slice(&reply_to);
            buf.extend_from_slice(&nw);
        },
        Some(lnk_rx_pid) => {
            let reply_to = lp.reply_to();
            let nw = lp.narrow_waist();
    // Link Pid
            buf.extend_from_slice(lnk_tx_sid.public_id().key().as_ref());
            trace!("ser link_tx_pk: \t\t{:?}", lnk_tx_sid.public_id().key().as_ref());
    // Link CC
            buf.extend_from_slice(lnk_tx_sid.public_id().chain_code().as_ref());
            trace!("ser link_cc_pk: \t\t{:?}", lnk_tx_sid.public_id().chain_code().as_ref());
    // Nonce
            let mut rng = rand::thread_rng();
            let nonce: Nonce = generate_nonce(&mut rng);
            buf.extend_from_slice(nonce.as_ref());
            trace!("ser link_nonce: \t\t{:?}", nonce.as_ref());
    // Tag
            let mut tag: Tag = [0; TAG_SIZE];
            let lnk_rx_pk = lnk_rx_pid.derive(&nonce);
            let lnk_tx_sk = lnk_tx_sid.derive(&nonce);
            let shared_secret = lnk_tx_sk.exchange(&lnk_rx_pk);
            let mut ctx = ChaCha20Poly1305::new(&shared_secret.as_ref(), &nonce, &[]);
            let (nws_size, mut nws) = serialize_narrow_waist_packet(&nw)?;
            let mut encrypted = vec![0u8; nws.len()];
            ctx.encrypt(&nws, &mut encrypted[..], &mut tag);
            nws.copy_from_slice(&encrypted[..]);
            buf.extend_from_slice(tag.as_ref());
            trace!("ser link_tag: \t\t\t{:?}", tag.as_ref());
    // Reply To Size
            let (reply_to_size, reply_to) = serialize_reply_to(&reply_to)?;
            buf.extend_from_slice(&[reply_to_size]);
            trace!("ser link_reply_to_size: \t{:?} actual_size: {}", [reply_to_size], reply_to.len());
    // Narrow Waist Size
            buf.extend_from_slice(&u16_to_u8(nws_size));
            trace!("ser nw_size: \t\t\t{:?} as_u16: {} actual {}", u16_to_u8(nws_size), nws_size, nws.len());
            buf.extend_from_slice(&reply_to);

    // Narrow Waist
            buf.extend_from_slice(&nws);
        },
    }
    Ok(buf)
}

pub fn deserialize_cyphertext_link_packet(data: &Vec<u8>, lnk_rx_sid: PrivateIdentity) -> Result<(PublicIdentity, LinkPacket)> {
// Link Pid
    let mut link_tx_pk = [0u8; ID_SIZE];
    link_tx_pk.clone_from_slice(&data[CYPHERTEXT_LINK_TX_PK_START..CYPHERTEXT_LINK_TX_PK_END]);
    trace!("des link_tx_pk: \t\t{:?}", link_tx_pk);
// Link CC
    let mut link_tx_cc = [0u8; CC_SIZE];
    //trace!("cc_size : {:?}, arr addresses {:?} {:?} {:?}", CC_SIZE, LINK_TX_CC_START, LINK_TX_CC_END,  LINK_TX_CC_END-LINK_TX_CC_START);
    link_tx_cc.clone_from_slice(&data[CYPHERTEXT_LINK_TX_CC_START..CYPHERTEXT_LINK_TX_CC_END]);
    trace!("des link_tx_cc: \t\t{:?}", link_tx_cc);
    let lnk_tx_pid: PublicIdentity = PublicIdentity::reconstitute(link_tx_pk, link_tx_cc);
// Nonce
    let mut link_nonce = [0u8; NONCE_SIZE];
    link_nonce.clone_from_slice(&data[CYPHERTEXT_LINK_NONCE_START..CYPHERTEXT_LINK_NONCE_END]);
    trace!("des link_nonce: \t\t{:?}", link_nonce);
// Tag
    let mut link_tag = [0u8; TAG_SIZE];
    link_tag.clone_from_slice(&data[CYPHERTEXT_LINK_TAG_START..CYPHERTEXT_LINK_TAG_END]);
    trace!("des link_tag: \t\t\t{:?}", link_tag);
// Reply To Length
    let reply_to_size = &data[CYPHERTEXT_LINK_REPLY_TO_SIZE_START..CYPHERTEXT_LINK_REPLY_TO_SIZE_END];
    trace!("des reply_to_size: \t\t{:?}", reply_to_size);
// Narrow Waist Length
    let mut nw_size = [0u8; 2];
    nw_size.clone_from_slice(&data[CYPHERTEXT_LINK_NARROW_WAIST_SIZE_START..CYPHERTEXT_LINK_NARROW_WAIST_SIZE_END]);
    trace!("des nw_size: \t\t\t{:?} as_u16: {}", nw_size, u8_to_u16(nw_size));
    let nw_size: usize = u8_to_u16(nw_size) as usize;
    let reply_to: ReplyTo = deserialize_reply_to(&data[CYPHERTEXT_LINK_NARROW_WAIST_SIZE_END..CYPHERTEXT_LINK_NARROW_WAIST_SIZE_END + reply_to_size[0] as usize].to_vec())?;
    let nw_start = CYPHERTEXT_LINK_NARROW_WAIST_SIZE_END + reply_to_size[0] as usize;
    let lnk_tx_pk = lnk_tx_pid.derive(&link_nonce);
    let lnk_rx_sk = lnk_rx_sid.derive(&link_nonce);
    let shared_secret = lnk_rx_sk.exchange(&lnk_tx_pk);
    let mut ctx = ChaCha20Poly1305::new(&shared_secret.as_ref(), &link_nonce, &[]);
    let nw: NarrowWaistPacket = match nw_size {
        CYPHERTEXT_NARROW_WAIST_PACKET_REQUEST_SIZE => {
            let mut decrypted = vec![0u8; nw_size];
            let encrypted = &data[nw_start..nw_start + nw_size];
            //trace!("des encrypted: actual_length: {} NARROW_WAIST_PACKET_ENCRYPTED_RESPONSE_SIZE {}\t\t\t{:?} ", encrypted.len(), CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_SIZE, encrypted);
            if !ctx.decrypt(encrypted, &mut decrypted, &link_tag) {
                return Err(anyhow!("failed to decrypt link packet"));
            };
            deserialize_cyphertext_narrow_waist_packet_request(&decrypted.to_vec())?
        },
        CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_SIZE => {
            let mut decrypted = vec![0u8; nw_size];
            let encrypted = &data[nw_start..nw_start + nw_size];
            //trace!("des encrypted: actual_length: {} NARROW_WAIST_PACKET_ENCRYPTED_RESPONSE_SIZE {}\t\t\t{:?} ", encrypted.len(), CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_SIZE, encrypted);
            if !ctx.decrypt(encrypted, &mut decrypted, &link_tag) {
                return Err(anyhow!("failed to decrypt link packet"));
            };
            deserialize_cyphertext_narrow_waist_packet_response(&decrypted.to_vec())?
        },
        _ => {
            return Err(anyhow!("Cyphertext packet arrived with an unrecognised NarrowWaistPacket SIZE of {}, where supported sizes are: {} or {}", nw_size, CYPHERTEXT_NARROW_WAIST_PACKET_REQUEST_SIZE, CYPHERTEXT_NARROW_WAIST_PACKET_RESPONSE_SIZE));
        },
    };
    Ok((lnk_tx_pid, LinkPacket::new(reply_to, nw)))
}
pub fn deserialize_cleartext_link_packet(data: &Vec<u8>) -> Result<(PublicIdentity, LinkPacket)> {
// Link Pid
    let mut link_tx_pk = [0u8; ID_SIZE];
    link_tx_pk.clone_from_slice(&data[CLEARTEXT_LINK_TX_PK_START..CLEARTEXT_LINK_TX_PK_END]);
    trace!("des link_tx_pk: \t\t{:?}", link_tx_pk);
// Link CC
    let mut link_tx_cc = [0u8; CC_SIZE];
    //trace!("cc_size : {:?}, arr addresses {:?} {:?} {:?}", CC_SIZE, LINK_TX_CC_START, LINK_TX_CC_END,  LINK_TX_CC_END-LINK_TX_CC_START);
    link_tx_cc.clone_from_slice(&data[CLEARTEXT_LINK_TX_CC_START..CLEARTEXT_LINK_TX_CC_END]);
    trace!("des link_tx_cc: \t\t{:?}", link_tx_cc);
    let lnk_tx_pid: PublicIdentity = PublicIdentity::reconstitute(link_tx_pk, link_tx_cc);
// Reply To Length
    let reply_to_size = &data[CLEARTEXT_LINK_REPLY_TO_SIZE_START..CLEARTEXT_LINK_REPLY_TO_SIZE_END];
    trace!("des reply_to_size: \t\t{:?}", reply_to_size);
// Narrow Waist Length
    let mut nw_size = [0u8; 2];
    nw_size.clone_from_slice(&data[CLEARTEXT_LINK_NARROW_WAIST_SIZE_START..CLEARTEXT_LINK_NARROW_WAIST_SIZE_END]);
    trace!("des nw_size: \t\t\t{:?} as_u16: {}", nw_size, u8_to_u16(nw_size));
    let nw_size: usize = u8_to_u16(nw_size) as usize;

    let reply_to: ReplyTo = deserialize_reply_to(&data[CLEARTEXT_LINK_NARROW_WAIST_SIZE_END..CLEARTEXT_LINK_NARROW_WAIST_SIZE_END + reply_to_size[0] as usize].to_vec())?;
    trace!("des reply_to: \t\t\t{:?}", reply_to);
    let nw_start = CLEARTEXT_LINK_NARROW_WAIST_SIZE_END + reply_to_size[0] as usize;
    let nw: NarrowWaistPacket = match nw_size {
        CLEARTEXT_NARROW_WAIST_PACKET_REQUEST_SIZE => {
            let cleartext_nw = &data[nw_start..nw_start + nw_size];
            trace!("des cleartext_nw: \t\t\t{:?}", cleartext_nw);
            deserialize_cleartext_narrow_waist_packet_request(&cleartext_nw.to_vec())?
        },
        CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_SIZE => {
            let cleartext_nw = &data[nw_start..nw_start + nw_size];
            deserialize_cleartext_narrow_waist_packet_response(&cleartext_nw.to_vec())?
        },
        _ => {
            return Err(anyhow!("Cleartext packet arrived with an unrecognised NarrowWaistPacket SIZE of {}, where supported sizes are: {} or {}", nw_size, CLEARTEXT_NARROW_WAIST_PACKET_REQUEST_SIZE, CLEARTEXT_NARROW_WAIST_PACKET_RESPONSE_SIZE));
        },
    };
    Ok((lnk_tx_pid, LinkPacket::new(reply_to, nw)))
}
pub fn deserialize_link_packet(data: &Vec<u8>, lnk_rx_sid: Option<PrivateIdentity>) -> Result<(PublicIdentity, LinkPacket)> {
    match lnk_rx_sid {
        Some(lnk_rx_sid) => {
            deserialize_cyphertext_link_packet(data, lnk_rx_sid)
        },
        None => {
            deserialize_cleartext_link_packet(data)
        },
    }
}
pub fn decode(msg: Vec<u8>, lnk_rx_sid: Option<PrivateIdentity>) -> Result<(PublicIdentity, LinkPacket)> {
    let dec = Decoder::new(6);
    let reconstituted: Vec<_> = msg.chunks(255).map(|c| Buffer::from_slice(c, c.len())).map(|d| dec.correct(&d,None).unwrap()).collect();
    let reconstituted: Vec<_> = reconstituted.iter().map(|d| d.data()).collect::<Vec<_>>().concat();
    Ok(deserialize_link_packet(&reconstituted, lnk_rx_sid)?)
}
pub fn encode(lp: LinkPacket, lnk_tx_sid: PrivateIdentity, lnk_rx_pid: Option<PublicIdentity>) -> Result<Vec<u8>> {
    let mut merged = vec![];
    let enc = Encoder::new(6);
    let nw: Vec<u8> = serialize_link_packet(&lp, lnk_tx_sid, lnk_rx_pid)?;
    let cs = nw.chunks(255-6);
    for c in cs {
        let c = enc.encode(&c[..]);
        merged.extend(&**c);
    }
    Ok(merged)
}
pub trait Link<'a> {
    fn run(&self) -> Result<()>;
    fn new(name: String, link: LinkId, router_in_and_out: ( Sender<InterLinkPacket> , Receiver<InterLinkPacket> ) ) -> Result<Self> where Self: Sized;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_u16_to_fro_u8() {
        let actual: u16 = u16::MIN;
        let expected: u16 = u8_to_u16(u16_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);

        let actual: u16 = 1;
        let expected: u16 = u8_to_u16(u16_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);

        let actual: u16 = u16::MAX;
        let expected: u16 = u8_to_u16(u16_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);
    }
    #[test]
    fn test_bfi_to_fro_u8() {
        let actual: BFI = [0u16; BLOOM_FILTER_INDEX_ELEMENT_LENGTH];
        let expected: BFI = u8_to_bfi(bfi_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);

        let actual: BFI = [0, 1, 2, 3];
        let expected: BFI = u8_to_bfi(bfi_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);

        let actual: BFI = [u16::MAX, u16::MAX, u16::MAX, u16::MAX];
        let expected: BFI = u8_to_bfi(bfi_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);
    }
    #[test]
    fn test_u64_to_fro_u8() {
        let actual: u64 = 0;
        let expected: u64 = u8_to_u64(u64_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);

        let actual: u64 = u64::MAX/2;
        let expected: u64 = u8_to_u64(u64_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);

        let actual: u64 = u64::MAX;
        let expected: u64 = u8_to_u64(u64_to_u8(actual));
        println!("expected: {:?}, actual: {:?}", expected, actual);
        assert_eq!(expected, actual);
    }
}
