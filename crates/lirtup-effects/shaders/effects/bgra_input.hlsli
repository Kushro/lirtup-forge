// ============================================================================
//  BGRA input view for the Magpie ports
//  ------------------------------------
//  Captured frames reach the presenter as BGRA bytes stored in RGBA8
//  textures (see also lut.hlsl). Magpie shaders expect RGB, so INPUT is
//  exposed through this view, which swaps red and blue on every read. The
//  swizzles are free (register renames), and the Magpie code that reads
//  INPUT stays unchanged. Writers must swap back with `.bgra`.
//
//  Distributed under the GPL-3.0 license.
// ============================================================================

Texture2D<float4> INPUT_BGRA : register(t0);

struct BGRAView {
  float4 SampleLevel(SamplerState s, float2 pos, float lod) {
    return INPUT_BGRA.SampleLevel(s, pos, lod).bgra;
  }
  float4 GatherRed(SamplerState s, float2 pos) {
    return INPUT_BGRA.GatherBlue(s, pos);
  }
  float4 GatherGreen(SamplerState s, float2 pos) {
    return INPUT_BGRA.GatherGreen(s, pos);
  }
  float4 GatherBlue(SamplerState s, float2 pos) {
    return INPUT_BGRA.GatherRed(s, pos);
  }
};

static BGRAView INPUT;
