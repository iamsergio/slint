// Copyright © SixtyFPS GmbH <info@slint.dev>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0

#[cfg(test)]
mod tests {
    use super::super::ffi::*;

    #[test]
    fn test_version_constant() {
        assert_eq!(IMPELLER_VERSION, 541081600);
    }

    #[test]
    fn test_struct_sizes() {
        assert_eq!(std::mem::size_of::<ImpellerRect>(), 16);
        assert_eq!(std::mem::size_of::<ImpellerPoint>(), 8);
        assert_eq!(std::mem::size_of::<ImpellerISize>(), 16);
    }

    #[test]
    fn test_color_construction() {
        let color = ImpellerColor {
            red: 1.0,
            green: 0.5,
            blue: 0.0,
            alpha: 1.0,
            color_space: ImpellerColorSpace::SRGB,
        };
        assert_eq!(color.red, 1.0);
        assert_eq!(color.color_space, ImpellerColorSpace::SRGB);
    }

    #[test]
    fn test_rect_construction() {
        let rect = ImpellerRect { x: 0.0, y: 0.0, width: 100.0, height: 200.0 };
        assert_eq!(rect.width, 100.0);
        assert_eq!(rect.height, 200.0);
    }
}
