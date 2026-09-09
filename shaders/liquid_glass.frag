#include <flutter/runtime_effect.glsl>
uniform vec2 u_size;
uniform float u_radius;
uniform float u_depth;
uniform sampler2D u_backdrop;
out vec4 frag_color;
void main() {
  vec2 pixel = FlutterFragCoord().xy;
  vec2 uv = pixel / u_size;
  vec2 halfSize = u_size * 0.5;
  float radius = min(u_radius, min(halfSize.x, halfSize.y));
  vec2 q = abs(pixel - halfSize) - halfSize + radius;
  float sd = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
  float edge = 1.0 - smoothstep(0.0, max(radius, 14.0), -sd);
  vec2 normal = normalize((pixel - halfSize) / max(halfSize, vec2(1.0)) + vec2(0.0001));
  vec2 bend = normal * edge * edge * u_depth / u_size;
  vec2 sampleUV = clamp(uv - bend, vec2(0.001), vec2(0.999));
#ifdef IMPELLER_TARGET_OPENGLES
  sampleUV.y = 1.0 - sampleUV.y;
#endif
  // Blur is composed outside this shader and interpolated across materials.
  frag_color = texture(u_backdrop, sampleUV);
}
