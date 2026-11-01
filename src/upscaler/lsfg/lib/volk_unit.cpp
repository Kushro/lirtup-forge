// SPDX-License-Identifier: MIT
//
// volk compiled as C++ in this extension, so the extension needs one compiler
// and one set of flags. volk's function pointers have hidden visibility here
// (-fvisibility=hidden) and never clash with libvulkan used by upscaler.vulkan.
#include "volk.c"
