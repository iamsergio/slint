// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

use std::env;
use std::path::PathBuf;

fn main() {
    let impeller_sdk_path = env::var("IMPELLER_SDK_PATH")
        .expect("IMPELLER_SDK_PATH environment variable must be set to the Impeller SDK installation directory");
    
    let sdk_path = PathBuf::from(impeller_sdk_path);
    
    println!("cargo:rustc-link-search=native={}/lib", sdk_path.display());
    println!("cargo:rustc-link-lib=dylib=impeller");
    
    println!("cargo:rerun-if-env-changed=IMPELLER_SDK_PATH");
}
