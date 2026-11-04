// ============================================================================
//  Split Compose
//  -------------
//  Side-by-side comparison on a frozen frame (live tuning): the left of the
//  split shows image A (the previous settings), the right shows image B (the
//  current settings). A thin vertical line and a rounded handle, both white
//  at `lineAlpha`, mark the split.
//
//  Parameters (pixels, in output coordinates):
//    - size        : output width and height.
//    - splitX      : x of the split (fractional; 0 = all B, width = all A).
//    - lineWidth   : width of the line.
//    - handleSize  : width and height of the handle, centred on the line.
//    - handleRadius: corner radius of the handle.
//    - lineAlpha   : opacity of the line and of the handle.
//
//  Distributed under the GPL-3.0 license.
// ============================================================================

Texture2D<float4> ImageA : register(t0);
Texture2D<float4> ImageB : register(t1);
[[vk::image_format("rgba8")]]
RWTexture2D<float4> Output : register(u0);

cbuffer SplitParams : register(b0) {
  uint2 size;
  float splitX;
  float lineWidth;
  float2 handleSize;
  float handleRadius;
  float lineAlpha;
};

// Signed distance to a rounded box of half extents `b` centred on 0.
float roundedBox(float2 p, float2 b, float r) {
  float2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

[numthreads(16, 16, 1)] void main(uint3 id : SV_DispatchThreadID) {
  if (id.x >= size.x || id.y >= size.y)
    return;

  float2 p = float2(id.xy) + 0.5;
  float4 color = p.x < splitX ? ImageA[id.xy] : ImageB[id.xy];

  // Line: antialiased band of `lineWidth` around the split.
  float bar = saturate(lineWidth * 0.5 - abs(p.x - splitX) + 0.5);

  // Handle: rounded box centred vertically on the line.
  float2 centre = float2(splitX, float(size.y) * 0.5);
  float d = roundedBox(p - centre, handleSize * 0.5, handleRadius);
  float handle = saturate(0.5 - d);

  // Two dark grips inside the handle, so it reads as draggable.
  float grip = 0.0;
  if (handle > 0.0) {
    float gx = abs(abs(p.x - splitX) - handleSize.x * 0.18);
    float gy = abs(p.y - centre.y) - handleSize.y * 0.25;
    grip = saturate(lineWidth * 0.5 - gx + 0.5) * saturate(0.5 - gy) * handle;
  }

  float cover = max(bar, handle) * lineAlpha;
  float3 rgb = lerp(color.rgb, float3(1.0, 1.0, 1.0), cover);
  rgb = lerp(rgb, float3(0.0, 0.0, 0.0), grip * 0.6);
  Output[id.xy] = float4(rgb, 1.0);
}
