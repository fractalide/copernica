
use libc;
use crate::sys;

use std::str;
use std::io;
use std::ffi::CStr;
use std::ffi::CString;


pub const SIOCSIFADDR: libc::c_ulong = 0x8020690c;
pub const SIOCGIFADDR: libc::c_ulong = 0xc0206921;
pub const SIOCSIFDSTADDR: libc::c_ulong = 0x8020690e;
pub const SIOCGIFDSTADDR: libc::c_ulong = 0xc0206922;
pub const SIOCSIFBRDADDR: libc::c_ulong = 0x80206913;
pub const SIOCGIFBRDADDR: libc::c_ulong = 0xc0206923;
pub const SIOCSIFFLAGS: libc::c_ulong = 0x80206910;
pub const SIOCGIFFLAGS: libc::c_ulong = 0xc0206911;
pub const SIOCSIFNETMASK: libc::c_ulong = 0x80206916;
pub const SIOCGIFNETMASK: libc::c_ulong = 0xc0206925;
pub const SIOCGIFMETRIC: libc::c_ulong = 0xc0206917;
pub const SIOCSIFMETRIC: libc::c_ulong = 0x80206918;
pub const SIOCGIFMTU: libc::c_ulong = 0xc0206933;
pub const SIOCSIFMTU: libc::c_ulong = 0x80206934;
pub const SIOCSIFMEDIA: libc::c_ulong = 0xc0206937;
pub const SIOCGIFMEDIA: libc::c_ulong = 0xc02c6938;
pub const SIOCGIFSTATUS: libc::c_ulong = 0xc331693d;
pub const SIOCSIFLLADDR: libc::c_ulong = 0x8020693c;


pub const RTF_LLDATA: libc::c_int = 0x400;
pub const RTF_DEAD: libc::c_int = 0x20000000;
pub const RTPRF_OURS: libc::c_int = libc::RTF_PROTO3;


#[repr(C)]
#[allow(non_snake_case)]
#[derive(Copy, Clone)]
pub union ifru {
    pub addr:      libc::sockaddr,
    pub dstaddr:   libc::sockaddr,
    pub broadaddr: libc::sockaddr,
    pub flags:     libc::c_short,
    pub metric:    libc::c_int,
    pub mtu:       libc::c_int,
    pub media:     libc::c_int,
    pub data:      *mut libc::c_void,
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ifreq {
    pub ifr_name: [libc::c_char; libc::IF_NAMESIZE],
    pub ifru: ifru,
}


#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rt_msghdr {
    pub rtm_msglen: libc::c_ushort, // to skip over non-understood messages
    pub rtm_version: libc::c_uchar, // future binary compatibility
    pub rtm_type: libc::c_uchar,    // message type 
    pub rtm_index: libc::c_ushort,  // index for associated ifp
    pub rtm_flags: libc::c_int,     // flags, incl. kern & message, e.g. DONE
    pub rtm_addrs: libc::c_int,     // bitmask identifying sockaddrs in msg
    pub rtm_pid: libc::pid_t,       // identify sender
    pub rtm_seq: libc::c_int,       // for sender to identify action
    pub rtm_errno: libc::c_int,     // why failed
    pub rtm_use: libc::c_int,       // from rtentry
    pub rtm_inits: u32,             // which metrics we are initializing
    pub rtm_rmx: rt_metrics,        // metrics themselves
}

// These numbers are used by reliable protocols for determining
// retransmission behavior and are included in the routing structure.
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rt_metrics {
    pub rmx_locks: u32,       // Kernel leaves these values alone
    pub rmx_mtu: u32,         // MTU for this path
    pub rmx_hopcount: u32,    // max hops expected
    pub rmx_expire: i32,      // lifetime for route, e.g. redirect
    pub rmx_recvpipe: u32,    // inbound delay-bandwidth product
    pub rmx_sendpipe: u32,    // outbound delay-bandwidth product
    pub rmx_ssthresh: u32,    // outbound gateway buffer limit
    pub rmx_rtt: u32,         // estimated round trip time
    pub rmx_rttvar: u32,      // estimated rtt variance
    pub rmx_pksent: u32,      // packets sent using this route
    pub rmx_state: u32,       // route state
    pub rmx_filler: [u32; 3], // will be used for T/TCP later
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rt_msghdr2 {
    pub rtm_msglen: libc::c_ushort,   // to skip over non-understood messages
    pub rtm_version: libc::c_uchar,   // future binary compatibility
    pub rtm_type: libc::c_uchar,      // message type 
    pub rtm_index: libc::c_ushort,    // index for associated ifp
    pub rtm_flags: libc::c_int,       // flags, incl. kern & message, e.g. DONE
    pub rtm_addrs: libc::c_int,       // bitmask identifying sockaddrs in msg
    pub rtm_refcnt: i32,              // reference count
    pub rtm_parentflags: libc::c_int, // which metrics we are initializing
    pub rtm_reserved: libc::c_int,    // metrics themselves
    pub rtm_use: libc::c_int,         // from rtentry
    pub rtm_inits: u32,               // which metrics we are initializing
    pub rtm_rmx: rt_metrics,          // metrics themselves
}


// Route reachability info
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rt_reach_info {
    pub ri_refcnt: u32,      // reference count
    pub ri_probes: u32,      // total # of probes
    pub ri_snd_expire: u64,  // tx expiration (calendar) time
    pub ri_rcv_expire: u64,  // rx expiration (calendar) time
    pub ri_rssi: i32,        // received signal strength
    pub ri_lqm: i32,         // link quality metric
    pub ri_npm: i32,         // node proximity metric
}

// Extended routing message header (private).
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rt_msghdr_ext {
    pub rtm_msglen: libc::c_ushort,   // to skip over non-understood messages
    pub rtm_version: libc::c_uchar,   // future binary compatibility
    pub rtm_type: libc::c_uchar,      // message type 
    pub rtm_index: u32,               // index for associated ifp
    pub rtm_flags: u32,               // flags, incl. kern & message, e.g. DONE
    pub rtm_reserved: u32,            // for future use
    pub rtm_addrs: u32,               // bitmask identifying sockaddrs in msg
    pub rtm_pid: libc::pid_t,         // identify sender
    pub rtm_seq: libc::c_int,         // for sender to identify action
    pub rtm_errno: libc::c_int,       // why failed
    pub rtm_use: u32,                 // from rtentry
    pub rtm_inits: u32,               // which metrics we are initializing
    pub rtm_rmx: rt_metrics,          // metrics themselves
    pub rtm_ri: rt_reach_info,        // route reachability info
}


// Routing statistics.
#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rtstat {
    pub rts_badredirect : libc::c_short, // bogus redirect calls
    pub rts_dynamic     : libc::c_short, // routes created by redirects
    pub rts_newgateway  : libc::c_short, // routes modified by redirects
    pub rts_unreach     : libc::c_short, // lookups which failed
    pub rts_wildcard    : libc::c_short, // lookups satisfied by a wildcard
    pub rts_badrtgwroute: libc::c_short, // route to gateway is not direct
}

#[allow(non_snake_case)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rt_addrinfo {
    pub rti_addrs: libc::c_int,
    pub rti_info : [ *mut libc::sockaddr; libc::RTAX_MAX as usize ],
}


pub fn if_name_to_index(ifname: &str) -> u32 {
    unsafe { sys::if_nametoindex(CString::new(ifname).unwrap().as_ptr()) }
}

pub fn if_name_to_mtu(name: &str) -> Result<usize, io::Error> {
    #[repr(C)]
    #[derive(Debug)]
    struct ifreq {
        ifr_name: [sys::c_char; sys::IF_NAMESIZE],
        ifr_mtu: sys::c_int
    }

    let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
    if fd == -1 {
        return Err(io::Error::last_os_error());
    }

    let mut ifreq = ifreq {
        ifr_name: [0; sys::IF_NAMESIZE],
        ifr_mtu: 0
    };
    for (i, byte) in name.as_bytes().iter().enumerate() {
        ifreq.ifr_name[i] = *byte as sys::c_char
    }
    
    let ret = unsafe { sys::ioctl(fd, sys::SIOCGIFMTU, &mut ifreq as *mut ifreq) };
    
    unsafe { libc::close(fd) };

    if ret == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ifreq.ifr_mtu as usize)
    }
}

pub fn if_index_to_name(ifindex: u32) -> String {
    let ifname_buf: [u8; libc::IF_NAMESIZE] = [0u8; libc::IF_NAMESIZE];
    unsafe {
        let ifname_cstr = CStr::from_bytes_with_nul_unchecked(&ifname_buf);
        let ptr = ifname_cstr.as_ptr() as *mut i8;
        libc::if_indextoname(ifindex, ptr);

        let mut pos = ifname_buf.len() - 1;
        while pos != 0 {
            if ifname_buf[pos] != 0 {
                if pos + 1 < ifname_buf.len() {
                    pos += 1;
                }
                break;
            }
            pos -= 1;
        }
        str::from_utf8(&ifname_buf[..pos]).unwrap().to_string()
    }
}

