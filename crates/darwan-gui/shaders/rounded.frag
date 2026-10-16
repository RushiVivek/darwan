#version 440
// A picture clipped to a rounded rectangle in one pass: a signed distance to the corners gives a one-pixel soft
// edge, so no offscreen layer or mask texture is needed. uvRect crops the texture the way PreserveAspectCrop would.
// With blur set it draws a stand-in instead: c0..c4 run softly into each other as a blurred picture would, and a
// light band crosses at `band` (0 to 1 across the width) while shine is up.
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;
    float radius;
    vec4 uvRect;
    float blur;
    float shine;
    float band;
    vec4 c0;
    vec4 c1;
    vec4 c2;
    vec4 c3;
    vec4 c4;
};
layout(binding = 1) uniform sampler2D source;

float blob(vec2 p, vec2 at) {
    vec2 d = p - at;
    return exp(-dot(d, d) * 5.0);
}

void main() {
    vec2 half_size = size * 0.5;
    vec2 q = abs(qt_TexCoord0 * size - half_size) - (half_size - vec2(radius));
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
    float inside = clamp(0.5 - d, 0.0, 1.0);
    if (blur < 0.5) {
        fragColor = texture(source, uvRect.xy + qt_TexCoord0 * uvRect.zw) * inside * qt_Opacity;
        return;
    }
    vec2 p = qt_TexCoord0;
    float w1 = blob(p, vec2(0.12, 0.18));
    float w2 = blob(p, vec2(0.88, 0.24));
    float w3 = blob(p, vec2(0.26, 0.92));
    float w4 = blob(p, vec2(0.80, 0.84));
    float w0 = 0.45;
    vec4 c = (c0 * w0 + c1 * w1 + c2 * w2 + c3 * w3 + c4 * w4) / (w0 + w1 + w2 + w3 + w4);
    float x = (p.x + 0.35 * p.y * size.y / size.x - band) / 0.22;
    c += vec4(1.0) * shine * 0.05 * exp(-x * x);
    // A whisper of grain, so the soft gradients don't band.
    c.rgb += (fract(sin(dot(p * size, vec2(12.9898, 78.233))) * 43758.5453) - 0.5) / 255.0 * c.a;
    fragColor = c * inside * qt_Opacity;
}
