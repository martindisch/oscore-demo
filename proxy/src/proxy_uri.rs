//! `Proxy-Uri implementation copied and modified from the `oscore` crate.

use std::collections::LinkedList;

/// Represents a split-up Proxy-Uri.
#[derive(Debug, PartialEq)]
pub struct ProxyUri {
    pub proxy_scheme: String,
    pub uri_host: String,
    pub uri_port: Option<String>,
    pub uri_path: Option<String>,
    pub uri_query: Option<String>,
}

impl From<&[u8]> for ProxyUri {
    /// Splits a Proxy-Uri into the Proxy-Scheme, Uri-Host, Uri-Port, Uri-Path
    /// and Uri-Query options.
    ///
    /// I don't implement this myself because I think I can do a better job
    /// than the 105 people wo have contributed to `rust-url`. On the contrary.
    /// I'd love to use it, but it requires `std`. And since I don't know of a
    /// better option, I have to write this abomination.
    fn from(bytes: &[u8]) -> ProxyUri {
        // Convert to a String we can work with
        let mut proxy_uri =
            String::from_utf8(bytes.to_vec()).expect("Proxy-Uri not UTF-8");

        // Take the Uri-Scheme out
        let scheme_end =
            proxy_uri.find(':').expect("No scheme end in Proxy-Uri");
        let proxy_scheme: String = proxy_uri.drain(..scheme_end).collect();
        // Drain the next three characters which should be '://'
        proxy_uri.drain(..3);

        // Take the Uri-Host out
        let host_end = if let Some(port_separator) = proxy_uri.find(':') {
            port_separator
        } else if let Some(path_separator) = proxy_uri.find('/') {
            path_separator
        } else if let Some(query_separator) = proxy_uri.find('?') {
            query_separator
        } else {
            proxy_uri.len()
        };
        let uri_host: String = proxy_uri.drain(..host_end).collect();

        // Take the Uri-Port out
        let port_end = if let Some(port_separator) = proxy_uri.find(':') {
            proxy_uri.remove(port_separator);
            if let Some(path_separator) = proxy_uri.find('/') {
                path_separator
            } else if let Some(query_separator) = proxy_uri.find('?') {
                query_separator
            } else {
                proxy_uri.len()
            }
        } else {
            0
        };
        let uri_port: String = proxy_uri.drain(..port_end).collect();
        // Now we can remove the leading path separator, if any
        if let Some(path_separator) = proxy_uri.find('/') {
            proxy_uri.remove(path_separator);
        }

        // Take the path out
        let path_end = if let Some(query_separator) = proxy_uri.find('?') {
            proxy_uri.remove(query_separator);
            query_separator
        } else {
            proxy_uri.len()
        };
        let uri_path: String = proxy_uri.drain(..path_end).collect();

        // Whatever remains is the query
        let uri_query = proxy_uri;

        ProxyUri {
            proxy_scheme,
            uri_host,
            uri_port: if uri_port.is_empty() {
                None
            } else {
                Some(uri_port)
            },
            uri_path: if uri_path.is_empty() {
                None
            } else {
                Some(uri_path)
            },
            uri_query: if uri_query.is_empty() {
                None
            } else {
                Some(uri_query)
            },
        }
    }
}

impl ProxyUri {
    /// Returns a `LinkedList` of the path components to be added as option
    /// values.
    pub fn get_path_list(&self) -> Option<LinkedList<Vec<u8>>> {
        match &self.uri_path {
            Some(uri_path) => Some(
                uri_path
                    .split('/')
                    .filter(|e| !e.is_empty())
                    .map(|s| s.as_bytes().to_vec())
                    .collect(),
            ),
            None => None,
        }
    }

    /// Returns a `LinkedList` of the query components to be added as option
    /// values.
    pub fn get_query_list(&self) -> Option<LinkedList<Vec<u8>>> {
        match &self.uri_query {
            Some(uri_query) => Some(
                uri_query
                    .split('&')
                    .filter(|e| !e.is_empty())
                    .map(|s| s.as_bytes().to_vec())
                    .collect(),
            ),
            None => None,
        }
    }
}
