// Copyright 2021 rust-ipfs-api Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//

use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("api returned error : `{}`", error))]
    Api { error: ipfs_api_prelude::ApiError },

    #[snafu(display("http_req error `{}`", error))]
    Http { error: http_req::error::Error },

    #[snafu(display("backend error `{}`", error))]
    Backend { error: http_req::error::Error },

    #[snafu(display("ipfs client error `{}`", error))]
    IpfsClientError { error: ipfs_api_prelude::Error },
}

impl From<ipfs_api_prelude::ApiError> for Error {
    fn from(err: ipfs_api_prelude::ApiError) -> Self {
        Error::Api { error: err }
    }
}

impl From<http_req::error::Error> for Error {
    fn from(err: http_req::error::Error) -> Self {
        Error::Http { error: err }
    }
}

// impl From<ipfs_api_prelude::Backend::Error> for Error {
//     fn from(err: ipfs_api_prelude::Backend::Error) -> Self {
//         Error::Backend { error: err }
//     }
// }
