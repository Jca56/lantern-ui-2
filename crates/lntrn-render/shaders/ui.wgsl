// Lantern 2D pass: one vertex stream for rects, rounded rects, strokes,
// glyphs and pictures, drawn against a single RGBA atlas (plus one image
// texture per run) with premultiplied blending.
//
// Coordinates arrive in framebuffer pixels (origin top-left, y down). Colors
// arrive LINEAR with straight alpha; the sRGB target encodes on write.

struct Uniforms {
    screen: vec2<f32>,
    // Atlas edge in texels: quads carry texel UVs so the atlas can grow.
    atlas: vec2<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;
// The picture of the current run (an sRGB texture: samples are linear).
@group(1) @binding(0) var image_tex: texture_2d<f32>;
@group(1) @binding(1) var image_samp: sampler;

// params.y modes
const MODE_FILL: f32 = 0.0;   // SDF rounded rect, filled
const MODE_GLYPH: f32 = 1.0;  // textured from the atlas
const MODE_STROKE: f32 = 2.0; // SDF rounded rect, inner stroke of width params.z
const MODE_PLAIN: f32 = 3.0;  // hard-edged quad, no SDF
const MODE_SHADOW: f32 = 4.0; // soft falloff outside the rect over params.z pixels
const MODE_IMAGE: f32 = 5.0;  // textured from the run's image, corners rounded by params.x

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) rect: vec4<f32>,   // center.xy, half_size.xy
    @location(4) params: vec4<f32>, // radius, mode, stroke width, image id
    @location(5) clip: vec4<f32>,   // x0, y0, x1, y1
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) rect: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) clip: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(in.pos.x / u.screen.x * 2.0 - 1.0, 1.0 - in.pos.y / u.screen.y * 2.0);
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    // Glyph quads carry texel coordinates; image quads carry 0..1.
    out.uv = select(in.uv / u.atlas, in.uv, in.params.y == MODE_IMAGE);
    out.color = in.color;
    out.rect = in.rect;
    out.params = in.params;
    out.clip = in.clip;
    return out;
}

// Signed distance from `p` to a rounded box centred at the origin.
fn sd_round_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Dark ink on a light ground dissolves when coverage is blended in linear
// light; thicken it by the ink's own lightness. Light ink is untouched.
fn coverage_gamma(rgb: vec3<f32>) -> f32 {
    let lin = clamp(dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
    let lightness = pow(lin, 1.0 / 2.2);
    return mix(1.0 / 1.6, 1.0, lightness);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let p = in.clip_pos.xy;
    if p.x < in.clip.x || p.y < in.clip.y || p.x >= in.clip.z || p.y >= in.clip.w {
        discard;
    }
    let mode = in.params.y;
    var color = in.color;

    if mode == MODE_GLYPH {
        // Atlas texels are premultiplied: text = white × coverage, emoji =
        // premultiplied color (drawn with a white tint). One multiply serves both.
        let texel = textureSample(atlas_tex, atlas_samp, in.uv);
        let cov = pow(texel.a, coverage_gamma(color.rgb));
        let boost = select(1.0, cov / texel.a, texel.a > 0.0);
        return vec4<f32>(color.rgb * color.a * texel.rgb * boost, color.a * cov);
    }

    if mode == MODE_IMAGE {
        let texel = textureSample(image_tex, image_samp, in.uv);
        var a = color.a * texel.a;
        if in.params.x > 0.0 {
            let d = sd_round_box(p - in.rect.xy, in.rect.zw, in.params.x);
            a = a * clamp(0.5 - d, 0.0, 1.0);
        }
        return vec4<f32>(color.rgb * texel.rgb * a, a);
    }

    var alpha = color.a;
    if mode == MODE_SHADOW {
        let d = sd_round_box(p - in.rect.xy, in.rect.zw, in.params.x);
        let blur = max(in.params.z, 1.0);
        let t = clamp(1.0 - (d + blur) / (2.0 * blur), 0.0, 1.0);
        alpha = alpha * t * t;
        return vec4<f32>(color.rgb * alpha, alpha);
    }
    if mode == MODE_FILL || mode == MODE_STROKE {
        let d = sd_round_box(p - in.rect.xy, in.rect.zw, in.params.x);
        var sd = d;
        if mode == MODE_STROKE {
            let w = in.params.z;
            sd = abs(d + w * 0.5) - w * 0.5;
        }
        // One-pixel anti-aliased edge.
        alpha = alpha * clamp(0.5 - sd, 0.0, 1.0);
    }
    return vec4<f32>(color.rgb * alpha, alpha);
}
