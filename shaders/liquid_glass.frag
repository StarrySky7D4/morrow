#include <flutter/runtime_effect.glsl>
uniform vec2 u_size;
uniform float u_radius;
uniform float u_depth;
uniform vec2 u_light;
uniform float u_press;
uniform sampler2D u_backdrop;
out vec4 frag_color;

void main() {
  vec2 pixel = FlutterFragCoord().xy;
  vec2 uv = pixel / u_size;
  vec2 halfSize = u_size * 0.5;
  float radius = clamp(u_radius, 0.0, min(halfSize.x, halfSize.y));
  vec2 centered = pixel - halfSize;
  vec2 q = abs(centered) - halfSize + radius;
  vec2 corner = max(q, vec2(0.0));
  float cornerLength = length(corner);
  float sd = cornerLength + min(max(q.x, q.y), 0.0) - radius;

  // Only the inner rim refracts. The center stays fixed for readable text.
  float edgeWidth = min(18.0, min(halfSize.x, halfSize.y) * 0.42);
  float edge = 1.0 - smoothstep(0.0, max(edgeWidth, 1.0), -sd);
  vec2 sideNormal = q.x > q.y
      ? vec2(sign(centered.x), 0.0)
      : vec2(0.0, sign(centered.y));
  vec2 normal = cornerLength > 0.001
      ? normalize(corner) * sign(centered)
      : sideNormal;
  float lightFacing = max(dot(normal, normalize(u_light + vec2(0.001))), 0.0);
  float depth = u_depth * (1.0 + 0.24 * u_press + 0.10 * lightFacing);
  vec2 bend = normal * edge * edge * depth / u_size;
  vec2 sampleUV = clamp(uv - bend, vec2(0.001), vec2(0.999));
#ifdef IMPELLER_TARGET_OPENGLES
  sampleUV.y = 1.0 - sampleUV.y;
#endif
  frag_color = texture(u_backdrop, sampleUV);
}
