// SPDX-License-Identifier: GPL-3.0-or-later
//
// lsfg_c.cpp: the C API of include/lsfg_c.h over the vendored lsfg-vk core
// (LSFG_3_1 / LSFG_3_1P) and the shader loader of Enigmata Temple. It replaces
// the CPython bridge of 1.x (src/upscaler/lsfg/lib/lsfg_module.cpp) call for
// call.
//
// Every C++ exception stops here: it becomes a negative LSFG_ERR_* code and a
// message for lsfg_last_error() (per thread). The global state (initialized,
// 3.1 or 3.1P) is guarded by a mutex, although the caller is expected to
// serialize the calls anyway (see the header).

#include "lsfg_c.h"

#include "common/exception.hpp"
#include "lsfg_3_1.hpp"
#include "lsfg_3_1p.hpp"
#include "shader_loader.h"

#include <algorithm>
#include <cstring>
#include <exception>
#include <memory>
#include <mutex>
#include <string>
#include <vector>

namespace {

std::mutex g_mutex;
bool g_initialized = false;
bool g_performance = false; // the backend chosen by lsfg_initialize: 3.1P

thread_local std::string g_last_error;

int fail(int code, std::string message) {
    g_last_error = message.empty() ? "unknown lsfg error" : std::move(message);
    return code;
}

// Run an lsfg call, turning exceptions into a code and a message.
template <typename F> int guarded(F &&fn) {
    try {
        return fn();
    } catch (const LSFG::vulkan_error &e) {
        return fail(LSFG_ERR_VULKAN, e.what());
    } catch (const std::bad_alloc &) {
        return fail(LSFG_ERR_INTERNAL, "out of memory");
    } catch (const std::exception &e) {
        return fail(LSFG_ERR_INTERNAL, e.what());
    } catch (...) {
        return fail(LSFG_ERR_INTERNAL, "unknown lsfg error");
    }
}

const char *shader_error(int32_t rc) {
    switch (rc) {
    case tlsfg::FgResult::DLL_UNREADABLE: return "the DLL cannot be read";
    case tlsfg::FgResult::MISSING_SPIRV: return "the frame generation shaders are missing";
    case tlsfg::FgResult::BINDING_REWRITE: return "the shaders cannot be rewritten";
    default: return "unknown shader loader error";
    }
}

int shader_code(int32_t rc) {
    return rc == tlsfg::FgResult::DLL_UNREADABLE ? LSFG_ERR_DLL : LSFG_ERR_SHADERS;
}

// extractShaders, with any exception from the PE parser as unreadable.
int32_t extract(const char *dll, tlsfg::ShaderSet &set) {
    try {
        return tlsfg::extractShaders(dll, set);
    } catch (...) {
        return tlsfg::FgResult::DLL_UNREADABLE;
    }
}

int require_initialized() {
    return g_initialized ? LSFG_OK : fail(LSFG_ERR_STATE, "lsfg is not initialized");
}

} // namespace

extern "C" {

int lsfg_probe(const char *dll, char *err, size_t errlen) {
    g_last_error.clear();
    std::string message;
    int rc;
    if (dll == nullptr) {
        rc = LSFG_ERR_ARGUMENT;
        message = "no DLL path";
    } else {
        tlsfg::ShaderSet set;
        const int32_t shaders = extract(dll, set);
        if (shaders == tlsfg::FgResult::OK) {
            rc = set.hasFp16() ? LSFG_PROBE_FP16 : LSFG_PROBE_FP32_ONLY;
        } else {
            rc = shader_code(shaders);
            message = std::string(dll) + ": " + shader_error(shaders);
        }
    }
    if (rc < 0) fail(rc, message);
    if (err != nullptr && errlen > 0) {
        const size_t n = std::min(message.size(), errlen - 1);
        std::memcpy(err, message.data(), n);
        err[n] = '\0';
    }
    return rc;
}

int lsfg_initialize(const char *dll, uint32_t vendor_id, uint32_t device_id,
                    uint32_t generated, float flow_scale, int performance,
                    int hdr, int fp16) {
    g_last_error.clear();
    std::lock_guard<std::mutex> lock(g_mutex);
    if (g_initialized) return fail(LSFG_ERR_STATE, "lsfg is already initialized; call lsfg_finalize()");
    if (dll == nullptr) return fail(LSFG_ERR_ARGUMENT, "no DLL path");
    if (generated < 1 || generated > 4) return fail(LSFG_ERR_ARGUMENT, "generated must be 1 to 4");

    auto set = std::make_shared<tlsfg::ShaderSet>();
    const int32_t shaders = extract(dll, *set);
    if (shaders != tlsfg::FgResult::OK)
        return fail(shader_code(shaders),
                    std::string("cannot read the shaders from ") + dll + " (" + shader_error(shaders) + ")");

    const uint64_t uuid = static_cast<uint64_t>(vendor_id) << 32 | device_id;
    // lsfg takes the inverse of the user's flow scale (as Temple and 1.x do)
    const float scale = 1.0f / std::clamp(flow_scale, 0.25f, 1.0f);
    const bool use_fp16 = fp16 != 0 && set->hasFp16();
    auto loader = tlsfg::makeLoader(set, use_fp16);
    const bool perf = performance != 0;
    const bool is_hdr = hdr != 0;
    const int rc = guarded([&] {
        if (perf)
            LSFG_3_1P::initialize(uuid, is_hdr, scale, generated, loader);
        else
            LSFG_3_1::initialize(uuid, is_hdr, scale, generated, loader);
        return LSFG_OK;
    });
    if (rc < 0) return rc;
    g_performance = perf;
    g_initialized = true;
    return use_fp16 ? LSFG_INIT_FP16 : 0;
}

int lsfg_create_context(int in0, int in1, const int *outs, size_t n,
                        uint32_t width, uint32_t height, uint32_t vk_format) {
    g_last_error.clear();
    std::lock_guard<std::mutex> lock(g_mutex);
    if (const int rc = require_initialized(); rc < 0) return rc;
    if (outs == nullptr || n == 0) return fail(LSFG_ERR_ARGUMENT, "no output images");
    if (width == 0 || height == 0) return fail(LSFG_ERR_ARGUMENT, "empty image size");
    const std::vector<int> out(outs, outs + n);
    const VkExtent2D extent{width, height};
    const VkFormat format = static_cast<VkFormat>(vk_format);
    return guarded([&] {
        const int32_t id = g_performance ? LSFG_3_1P::createContext(in0, in1, out, extent, format)
                                         : LSFG_3_1::createContext(in0, in1, out, extent, format);
        return id < 0 ? fail(LSFG_ERR_INTERNAL, "lsfg returned an invalid context id") : static_cast<int>(id);
    });
}

int lsfg_present(int ctx, int in_sem, const int *out_sems, size_t n) {
    g_last_error.clear();
    std::lock_guard<std::mutex> lock(g_mutex);
    if (const int rc = require_initialized(); rc < 0) return rc;
    if (ctx < 0) return fail(LSFG_ERR_ARGUMENT, "invalid context id");
    if (out_sems == nullptr && n > 0) return fail(LSFG_ERR_ARGUMENT, "no output semaphores");
    const std::vector<int> out(out_sems, out_sems + n);
    return guarded([&] {
        if (g_performance)
            LSFG_3_1P::presentContext(ctx, in_sem, out);
        else
            LSFG_3_1::presentContext(ctx, in_sem, out);
        return LSFG_OK;
    });
}

int lsfg_delete_context(int ctx) {
    g_last_error.clear();
    std::lock_guard<std::mutex> lock(g_mutex);
    if (const int rc = require_initialized(); rc < 0) return rc;
    if (ctx < 0) return fail(LSFG_ERR_ARGUMENT, "invalid context id");
    return guarded([&] {
        if (g_performance)
            LSFG_3_1P::deleteContext(ctx);
        else
            LSFG_3_1::deleteContext(ctx);
        return LSFG_OK;
    });
}

void lsfg_finalize(void) {
    g_last_error.clear();
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_initialized) return;
    const bool perf = g_performance;
    g_initialized = false;
    guarded([&] {
        if (perf)
            LSFG_3_1P::finalize();
        else
            LSFG_3_1::finalize();
        return LSFG_OK;
    });
}

const char *lsfg_last_error(void) {
    return g_last_error.c_str();
}

} // extern "C"
