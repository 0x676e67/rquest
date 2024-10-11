//! DNS resolution via the [hickory-resolver](https://github.com/hickory-dns/hickory-dns) crate

use crate::error::Kind;
use crate::Error;

use super::{Addrs, Name, Resolve, Resolving};
use hickory_resolver::{
    config::LookupIpStrategy, lookup_ip::LookupIpIntoIter, system_conf, TokioAsyncResolver,
};
use std::io;
use std::net::SocketAddr;

/// Wrapper around an `AsyncResolver`, which implements the `Resolve` trait.
#[derive(Debug, Clone)]
pub(crate) struct HickoryDnsResolver {
    /// Since we might not have been called in the context of a
    /// Tokio Runtime in initialization, so we must delay the actual
    /// construction of the resolver.
    state: TokioAsyncResolver,
}

impl HickoryDnsResolver {
    /// Create a new resolver with the default configuration,
    /// which reads from `/etc/resolve.conf`. The options are
    /// overriden to look up for both IPv4 and IPv6 addresses
    /// to work with "happy eyeballs" algorithm.
    pub(crate) fn new(strategy: Option<LookupIpStrategy>) -> crate::Result<Self> {
        let (config, mut opts) = system_conf::read_system_conf()
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("error reading DNS system conf: {}", e),
                )
            })
            .map_err(|err| Error::new(Kind::Builder, Some(err.to_string())))?;
        opts.ip_strategy = strategy.unwrap_or(LookupIpStrategy::Ipv4AndIpv6);
        Ok(Self {
            state: TokioAsyncResolver::tokio(config, opts),
        })
    }
}

struct SocketAddrs {
    iter: LookupIpIntoIter,
}

impl Resolve for HickoryDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let resolver = self.clone();
        Box::pin(async move {
            let lookup = resolver.state.lookup_ip(name.as_str()).await?;
            let addrs: Addrs = Box::new(SocketAddrs {
                iter: lookup.into_iter(),
            });
            Ok(addrs)
        })
    }
}

impl Iterator for SocketAddrs {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|ip_addr| SocketAddr::new(ip_addr, 0))
    }
}
