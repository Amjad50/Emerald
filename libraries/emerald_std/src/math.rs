#[allow(unused_macros)]
macro_rules! no_mangle {
    ($(fn $fun:ident($($iid:ident : $ity:ty),+) -> $oty:ty;)+) => {
        $(
            #[linkage="weak"]
            #[no_mangle]
            pub unsafe extern "C" fn $fun($($iid: $ity),+) -> $oty {
                libm::$fun($($iid),+)
            }
        )+
    }
}

no_mangle! {
    fn acos(n: f64) -> f64;
    fn acosf(n: f32) -> f32;
    fn asin(n: f64) -> f64;
    fn asinf(n: f32) -> f32;
    fn atan(n: f64) -> f64;
    fn atan2(a: f64, b: f64) -> f64;
    fn atan2f(a: f32, b: f32) -> f32;
    fn atanf(n: f32) -> f32;
    fn cbrt(n: f64) -> f64;
    fn cbrtf(n: f32) -> f32;
    fn cosh(n: f64) -> f64;
    fn coshf(n: f32) -> f32;
    fn expm1(n: f64) -> f64;
    fn expm1f(n: f32) -> f32;
    fn fdim(a: f64, b: f64) -> f64;
    fn fdimf(a: f32, b: f32) -> f32;
    fn hypot(x: f64, y: f64) -> f64;
    fn hypotf(x: f32, y: f32) -> f32;
    fn log1p(n: f64) -> f64;
    fn log1pf(n: f32) -> f32;
    fn sinh(n: f64) -> f64;
    fn sinhf(n: f32) -> f32;
    fn tan(n: f64) -> f64;
    fn tanf(n: f32) -> f32;
    fn tanh(n: f64) -> f64;
    fn tanhf(n: f32) -> f32;
    fn tgamma(n: f64) -> f64;
    fn tgammaf(n: f32) -> f32;
    // fn lgamma_r(n: f64, s: &mut i32) -> f64;
    // fn lgammaf_r(n: f32, s: &mut i32) -> f32;
    fn cos(x: f64) -> f64;
    fn expf(x: f32) -> f32;
    fn log2(x: f64) -> f64;
    fn log2f(x: f32) -> f32;
    fn log10(x: f64) -> f64;
    fn log10f(x: f32) -> f32;
    fn log(x: f64) -> f64;
    fn logf(x: f32) -> f32;
    fn fmin(x: f64, y: f64) -> f64;
    fn fminf(x: f32, y: f32) -> f32;
    fn fmax(x: f64, y: f64) -> f64;
    fn fmaxf(x: f32, y: f32) -> f32;
    fn round(x: f64) -> f64;
    fn roundf(x: f32) -> f32;
    fn rint(x: f64) -> f64;
    fn rintf(x: f32) -> f32;
    fn sin(x: f64) -> f64;
    fn pow(x: f64, y: f64) -> f64;
    fn powf(x: f32, y: f32) -> f32;
    fn fmod(x: f64, y: f64) -> f64;
    fn fmodf(x: f32, y: f32) -> f32;
    fn ldexp(f: f64, n: i32) -> f64;
    fn ldexpf(f: f32, n: i32) -> f32;
    fn cosf(x: f32) -> f32;
    fn exp(x: f64) -> f64;
    fn sinf(x: f32) -> f32;
    fn exp2(x: f64) -> f64;
    fn exp2f(x: f32) -> f32;
    fn fma(x: f64, y: f64, z: f64) -> f64;
    fn fmaf(x: f32, y: f32, z: f32) -> f32;
    fn sqrtf(x: f32) -> f32;
    fn sqrt(x: f64) -> f64;
    fn ceil(x: f64) -> f64;
    fn ceilf(x: f32) -> f32;
    fn floor(x: f64) -> f64;
    fn floorf(x: f32) -> f32;
    fn trunc(x: f64) -> f64;
    fn truncf(x: f32) -> f32;
}

#[linkage = "weak"]
#[no_mangle]
pub unsafe extern "C" fn lgamma_r(x: f64, s: &mut i32) -> f64 {
    let r = libm::lgamma_r(x);
    *s = r.1;
    r.0
}

#[linkage = "weak"]
#[no_mangle]
pub unsafe extern "C" fn lgammaf_r(x: f32, s: &mut i32) -> f32 {
    let r = libm::lgammaf_r(x);
    *s = r.1;
    r.0
}
