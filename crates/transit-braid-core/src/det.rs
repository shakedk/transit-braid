//! Deterministic math: the same bits on every platform.
//!
//! The standard library's `cos`, `exp` and friends come from the platform's math library, and
//! native builds and WebAssembly can differ in the last bit. The pipeline makes yes/no decisions
//! on computed numbers (is this within 5 m? is this move accepted?), so a last-bit difference can
//! change its output by meters, and a native build and the browser would disagree. These functions
//! use only `+ − × ÷` (exact in IEEE 754 everywhere), so they give identical results on every
//! machine, within a couple of units in the last place of the true value.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

/// Cosine, for any finite `x` (radians). Accurate to about 1e-16 on `[-π, π]`.
#[must_use]
pub fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    // Into [-π, π], then [0, π] (cos is even), then [0, π/2] (cos(π − a) = −cos a).
    let mut a = x - TAU * (x / TAU).round();
    a = a.abs();
    if a > FRAC_PI_2 {
        -cos_small(PI - a)
    } else {
        cos_small(a)
    }
}

/// Cosine on `[0, π/2]`: its Taylor series to x²² in Horner form (error below 2e-19 there).
fn cos_small(x: f64) -> f64 {
    // (−1)ⁿ / (2n)! for n = 11 down to 1.
    #[expect(
        clippy::lossy_float_literal,
        reason = "22! is written exactly; it rounds to the nearest double, the value these bits always had"
    )]
    const C: [f64; 11] = [
        -1.0 / 1_124_000_727_777_607_680_000.0,
        1.0 / 2_432_902_008_176_640_000.0,
        -1.0 / 6_402_373_705_728_000.0,
        1.0 / 20_922_789_888_000.0,
        -1.0 / 87_178_291_200.0,
        1.0 / 479_001_600.0,
        -1.0 / 3_628_800.0,
        1.0 / 40_320.0,
        -1.0 / 720.0,
        1.0 / 24.0,
        -1.0 / 2.0,
    ];
    let x2 = x * x;
    let mut p = 0.0;
    for c in C {
        p = p * x2 + c;
    }
    p * x2 + 1.0
}

/// 1/n! for n = 0 to 13.
const INV_FACTORIAL: [f64; 14] = [
    1.0,
    1.0,
    1.0 / 2.0,
    1.0 / 6.0,
    1.0 / 24.0,
    1.0 / 120.0,
    1.0 / 720.0,
    1.0 / 5_040.0,
    1.0 / 40_320.0,
    1.0 / 362_880.0,
    1.0 / 3_628_800.0,
    1.0 / 39_916_800.0,
    1.0 / 479_001_600.0,
    1.0 / 6_227_020_800.0,
];

/// e^x, for any `x`: 0 below about −745, infinity above about 709.8. Accurate to about 2e-16
/// relative.
#[must_use]
pub fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x > 709.782_712_893_384 {
        return f64::INFINITY;
    }
    if x < -745.133_219_101_941_1 {
        return 0.0;
    }
    // x = k·ln 2 + r with |r| ≤ ln 2 / 2; ln 2 in two parts so k·ln2_hi is exact.
    // ln 2 = LN2_HI + LN2_LO, the high part with its low 32 bits zero (as exact bit patterns).
    const LN2_HI: f64 = f64::from_bits(0x3FE6_2E42_FEE0_0000);
    const LN2_LO: f64 = f64::from_bits(0x3DEA_39EF_3579_3C76);
    let k = (x * std::f64::consts::LOG2_E).round();
    let r = (x - k * LN2_HI) - k * LN2_LO;
    // e^r: its Taylor series to r¹³ (error below 4e-18 for |r| ≤ 0.35).
    let mut p = INV_FACTORIAL[13];
    for c in INV_FACTORIAL[..13].iter().rev() {
        p = p * r + c;
    }
    scale(p, k as i32)
}

/// (ln 2)ⁿ / n! for n = 0 to 17, each the nearest double to the exact value: 2^f's Taylor
/// series in f, since 2^f = e^(f·ln 2).
const EXP2_TAYLOR: [f64; 18] = [
    1.0,
    std::f64::consts::LN_2,
    0.240_226_506_959_100_72,
    0.055_504_108_664_821_58,
    0.009_618_129_107_628_477,
    0.001_333_355_814_642_844_3,
    0.000_154_035_303_933_816_1,
    1.525_273_380_405_984_1e-5,
    1.321_548_679_014_431e-6,
    1.017_808_600_923_97e-7,
    7.054_911_620_801_123e-9,
    4.445_538_271_870_811_6e-10,
    2.567_843_599_348_820_6e-11,
    1.369_148_885_390_412_8e-12,
    6.778_726_354_822_545e-14,
    3.132_436_707_088_428_7e-15,
    1.357_024_794_875_514_8e-16,
    5.533_046_532_458_242e-18,
];

/// 2^x, for any `x`: exactly 2^n at every whole `x = n`, 0 below about −1075, infinity from 1024
/// up. Within about 1.3 units in the last place of the exact value in between, and it never
/// decreases as `x` grows. The pipeline turns a map zoom into a scale with it.
#[must_use]
pub fn exp2(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x >= 1024.0 {
        return f64::INFINITY;
    }
    if x < -1080.0 {
        return 0.0;
    }
    // x = n + f with n whole and 0 ≤ f < 1. f = x − n is exact (n is 0, or x and n are within a
    // factor of two), except for −½ < x < 0: there it is rounded, by under 6e-17, and if it rounds
    // up to 1, that is 2^(n + 1).
    let mut n = x.floor();
    let mut f = x - n;
    if f >= 1.0 {
        n += 1.0;
        f = 0.0;
    }
    // 2^f by its Taylor series in f to f¹⁷, in Horner form (the terms left out add up to under
    // 3e-19). It is exactly 1 at f = 0. Every coefficient and f are 0 or more, so each rounded
    // step, and the sum, never decreases as f grows; and at the largest f below 1 the sum is at
    // most 2 (a test checks it), so it never decreases from one whole n to the next either.
    let mut p = EXP2_TAYLOR[17];
    for c in EXP2_TAYLOR[..17].iter().rev() {
        p = p * f + c;
    }
    scale(p, n as i32)
}

/// `v · 2^k`, exactly (2^k built from its bits; in two steps near the ends of the range).
fn scale(v: f64, k: i32) -> f64 {
    let pow2 = |e: i32| f64::from_bits(((e + 1023) as u64) << 52);
    if k > 1023 {
        v * pow2(1023) * pow2(k - 1023)
    } else if k < -1022 {
        v * pow2(-1022) * pow2((k + 1022).max(-1022))
    } else {
        v * pow2(k)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(f64::MIN_POSITIVE)
    }

    #[test]
    fn cos_matches_known_values() {
        assert_eq!(cos(0.0), 1.0);
        assert!(close(cos(PI / 3.0), 0.5, 1e-15));
        assert!(cos(FRAC_PI_2).abs() < 1e-16);
        assert!(close(cos(PI), -1.0, 1e-16));
        assert!(close(cos(-PI / 6.0), 3.0_f64.sqrt() / 2.0, 1e-15));
        assert!(close(cos(TAU + 0.5), 0.5_f64.cos(), 1e-14));
        assert!(cos(f64::NAN).is_nan() && cos(f64::INFINITY).is_nan());
    }

    #[test]
    fn exp_matches_known_values_and_its_ends() {
        assert_eq!(exp(0.0), 1.0);
        assert!(close(exp(1.0), std::f64::consts::E, 2e-16));
        assert!(close(exp(-1.0), 1.0 / std::f64::consts::E, 2e-16));
        assert!(close(exp(-5.703_782_474_656_201), 0.01 / 3.0, 1e-15));
        assert_eq!(exp(-800.0), 0.0);
        assert_eq!(exp(800.0), f64::INFINITY);
        assert!(exp(f64::NAN).is_nan());
        assert_eq!(exp(f64::NEG_INFINITY), 0.0);
        assert!(
            close(exp(-744.0), (-744.0_f64).exp(), 1e-10),
            "subnormal range"
        );
        assert!(close(exp(709.0), 709.0_f64.exp(), 1e-15));
    }

    /// 2^n from its bits, for every whole n with a finite, non-zero 2^n (−1074 to 1023).
    fn pow2_bits(n: i32) -> f64 {
        if n >= -1022 {
            f64::from_bits(((n + 1023) as u64) << 52)
        } else {
            f64::from_bits(1 << (n + 1074))
        }
    }

    /// How many doubles apart two positive finite numbers are.
    fn ulps(a: f64, b: f64) -> u64 {
        a.to_bits().abs_diff(b.to_bits())
    }

    #[test]
    fn exp2_is_exact_at_every_whole_number() {
        for n in -1074..=1023 {
            assert_eq!(exp2(f64::from(n)), pow2_bits(n), "2^{n}");
        }
        assert_eq!(exp2(-0.0), 1.0);
        // The zooms the pipeline takes: 0 to 24.
        for z in 0..=24 {
            assert_eq!(exp2(f64::from(z)), f64::from(1_u32 << z), "zoom {z}");
        }
    }

    #[test]
    fn exp2_ends_and_special_values() {
        assert!(exp2(f64::NAN).is_nan());
        assert_eq!(exp2(f64::INFINITY), f64::INFINITY);
        assert_eq!(exp2(f64::NEG_INFINITY), 0.0);
        assert_eq!(exp2(1024.0), f64::INFINITY);
        assert_eq!(exp2(f64::MAX), f64::INFINITY);
        assert_eq!(exp2(-f64::MAX), 0.0);
        // The largest x below 1024 still gives a finite number, just under f64::MAX.
        let top = exp2(1024.0_f64.next_down());
        assert!(top.is_finite() && top > pow2_bits(1023));
        // Into the subnormals: 2^-1074 is the smallest double; 2^-1075 is a tie, rounded to 0.
        assert_eq!(exp2(-1074.0), f64::from_bits(1));
        assert_eq!(exp2(-1074.5), f64::from_bits(1));
        assert_eq!(exp2(-1075.0), 0.0);
        assert_eq!(exp2(-1080.0), 0.0);
        for x in [-1060.25, -1030.7, -1022.5] {
            assert!(ulps(exp2(x), x.exp2()) <= 1, "subnormal {x}");
        }
        // Just below 0 the fraction x + 1 rounds; it stays at most 1, and is 1 when it rounds up.
        assert_eq!(exp2(-1e-300), 1.0);
        assert_eq!(exp2(-f64::EPSILON / 8.0), 1.0);
        assert!(exp2(-f64::EPSILON / 2.0) <= 1.0);
        assert!(exp2(-0.25) < 1.0 && exp2(-0.25) > 0.8);
    }

    #[test]
    fn exp2_taylor_coefficients_are_ln2_powers_over_factorials() {
        let mut c = 1.0;
        for (k, &want) in EXP2_TAYLOR.iter().enumerate() {
            if k > 0 {
                c = c * std::f64::consts::LN_2 / k as f64;
            }
            assert!(
                close(want, c, 4.0 * k as f64 * f64::EPSILON),
                "term {k}: {want} vs {c}"
            );
        }
    }

    #[test]
    fn exp2_is_within_one_ulp_of_the_nearest_double_to_the_exact_value() {
        // The nearest doubles to 2^f, from 60-digit decimals.
        let exact = [
            (0.125, 0x3FF1_72B8_3C7D_517B_u64),
            (0.25, 0x3FF3_06FE_0A31_B715),
            (0.37, 0x3FF4_AD7A_2967_B268),
            (0.5, 0x3FF6_A09E_667F_3BCD),
            (0.75, 0x3FFA_E89F_995A_D3AD),
            (0.9, 0x3FFD_DB68_0117_AB12),
        ];
        for (f, bits) in exact {
            assert!(
                ulps(exp2(f), f64::from_bits(bits)) <= 1,
                "2^{f} = {}",
                exp2(f)
            );
        }
        // IEEE sqrt is the nearest double to the exact root everywhere: 2^(n + ½) = √2 · 2^n.
        let root2 = 2.0_f64.sqrt();
        for n in -1020..=1020 {
            let got = exp2(f64::from(n) + 0.5);
            assert!(ulps(got, root2 * pow2_bits(n)) <= 1, "2^{n}.5 = {got}");
        }
    }

    #[test]
    fn exp2_never_decreases_across_whole_numbers() {
        // The proof's one numeric step: at the largest fraction below 1 the sum is at most 2.
        let top = 1.0_f64.next_down();
        assert!(exp2(top) <= 2.0, "{}", exp2(top));
        for n in -1073..=1023 {
            let x = f64::from(n);
            let below = exp2(x.next_down());
            assert!(below <= exp2(x) && exp2(x) <= exp2(x.next_up()), "at {n}");
            assert!(below >= exp2(x - 1.0), "at {n}");
        }
    }

    #[test]
    fn exp2_over_the_zoom_range_never_decreases_and_is_within_two_ulps_of_the_platform() {
        // 2.4 million zooms from 0 to 24 (an odd step, so the fractions are not round binary
        // numbers), and the two neighbours of each tenth of a zoom.
        let step = 24.0 / 2_400_007.0;
        let mut last = 0.0;
        let mut worst = (0, 0.0);
        for k in 0..=2_400_007_u32 {
            let z = (f64::from(k) * step).min(24.0);
            let got = exp2(z);
            assert!(got >= last, "decreases at zoom {z}");
            last = got;
            let off = ulps(got, z.exp2());
            if off > worst.0 {
                worst = (off, z);
            }
        }
        assert!(worst.0 <= 2, "{} ulps at zoom {}", worst.0, worst.1);
        for tenth in 0..=240 {
            let z = f64::from(tenth) / 10.0;
            let (a, b) = (z.next_down().max(0.0), z.next_up().min(24.0));
            assert!(exp2(a) <= exp2(z) && exp2(z) <= exp2(b), "around zoom {z}");
        }
    }

    proptest! {
        #[test]
        fn exp2_agrees_with_the_platform_within_two_ulps(x in -1022.0..1023.99_f64) {
            prop_assert!(ulps(exp2(x), x.exp2()) <= 2, "{} vs {}", exp2(x), x.exp2());
        }

        #[test]
        fn exp2_never_decreases(a in -60.0..60.0_f64, b in -60.0..60.0_f64) {
            let (lo, hi) = (a.min(b), a.max(b));
            prop_assert!(exp2(lo) <= exp2(hi), "2^{} > 2^{}", lo, hi);
            prop_assert!(exp2(lo) <= exp2(lo.next_up()), "at {}", lo);
        }

        #[test]
        fn cos_agrees_with_the_platform_within_a_few_ulps(x in -10.0..10.0_f64) {
            prop_assert!((cos(x) - x.cos()).abs() <= 4.0 * f64::EPSILON, "{} vs {}", cos(x), x.cos());
        }

        #[test]
        fn exp_agrees_with_the_platform_within_a_few_ulps(x in -700.0..700.0_f64) {
            prop_assert!(close(exp(x), x.exp(), 4.0 * f64::EPSILON), "{} vs {}", exp(x), x.exp());
        }
    }
}
