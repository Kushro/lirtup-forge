// SPDX-License-Identifier: GPL-3.0-or-later
//
// Python bindings for the lsfg-vk frame generation core (third_party/lsfg-vk).
//
// LSFG runs on its own VkDevice (created through volk) on the same GPU as the
// upscaler. Frames are shared as opaque fds: the upscaler exports two input
// images, one output image per generated frame and the semaphores; LSFG
// imports them (and takes ownership of each fd).
//
// The shaders come from the user's Lossless.dll, read in memory by
// shader_loader.cpp (from Enigmata Temple).
//
//   probe(dll) -> str
//   initialize(dll, vendor_id, device_id, generated, flow_scale, performance,
//              fp16) -> None
//   create_context(in0_fd, in1_fd, out_fds, width, height, vk_format) -> int
//   present(context, in_semaphore_fd, out_semaphore_fds) -> None
//   delete_context(context) -> None
//   finalize() -> None

#define PY_SSIZE_T_CLEAN
#include <Python.h>

#include "lsfg_3_1.hpp"
#include "lsfg_3_1p.hpp"
#include "shader_loader.h"

#include <algorithm>
#include <exception>
#include <memory>
#include <string>
#include <vector>

namespace {

// The backend chosen by initialize() (3.1, or 3.1P for performance mode)
bool g_performance = false;
bool g_initialized = false;

PyObject *g_error = nullptr; // lsfg.LsfgError

const char *shader_error(int32_t rc) {
    switch (rc) {
    case tlsfg::FgResult::DLL_UNREADABLE: return "dll_unreadable";
    case tlsfg::FgResult::MISSING_SPIRV: return "missing_spirv";
    case tlsfg::FgResult::BINDING_REWRITE: return "binding_rewrite";
    default: return "unknown";
    }
}

bool read_fds(PyObject *list, std::vector<int> &out) {
    PyObject *seq = PySequence_Fast(list, "expected a list of file descriptors");
    if (!seq) return false;
    const Py_ssize_t n = PySequence_Fast_GET_SIZE(seq);
    for (Py_ssize_t i = 0; i < n; i++) {
        long fd = PyLong_AsLong(PySequence_Fast_GET_ITEM(seq, i));
        if (fd == -1 && PyErr_Occurred()) {
            Py_DECREF(seq);
            return false;
        }
        out.push_back(static_cast<int>(fd));
    }
    Py_DECREF(seq);
    return true;
}

// Run an LSFG call without the GIL, turning C++ exceptions into LsfgError.
template <typename F> bool call(F &&fn) {
    std::string error;
    Py_BEGIN_ALLOW_THREADS;
    try {
        fn();
    } catch (const std::exception &e) {
        error = e.what();
        if (error.empty()) error = "unknown LSFG error";
    } catch (...) {
        error = "unknown LSFG error";
    }
    Py_END_ALLOW_THREADS;
    if (!error.empty()) {
        PyErr_SetString(g_error, error.c_str());
        return false;
    }
    return true;
}

// extractShaders, with any C++ exception from the PE parser as unreadable
int32_t extract(const char *dll, tlsfg::ShaderSet &set) {
    int32_t rc;
    Py_BEGIN_ALLOW_THREADS;
    try {
        rc = tlsfg::extractShaders(dll, set);
    } catch (...) {
        rc = tlsfg::FgResult::DLL_UNREADABLE;
    }
    Py_END_ALLOW_THREADS;
    return rc;
}

bool require_initialized() {
    if (!g_initialized) {
        PyErr_SetString(g_error, "LSFG is not initialized");
        return false;
    }
    return true;
}

// ---------------------------------------------------------------------------

PyObject *lsfg_probe(PyObject *, PyObject *args) {
    const char *dll;
    if (!PyArg_ParseTuple(args, "s", &dll)) return nullptr;
    tlsfg::ShaderSet set;
    const int32_t rc = extract(dll, set);
    if (rc != tlsfg::FgResult::OK) return PyUnicode_FromString(shader_error(rc));
    return PyUnicode_FromString(set.hasFp16() ? "ok" : "ok_fp32");
}

PyObject *lsfg_initialize(PyObject *, PyObject *args, PyObject *kwds) {
    static const char *kwlist[] = {"dll",        "vendor_id",   "device_id",
                                   "generated",  "flow_scale",  "performance",
                                   "fp16",       nullptr};
    const char *dll;
    unsigned int vendor_id, device_id, generated;
    double flow_scale = 1.0;
    int performance = 0, fp16 = 1;
    if (!PyArg_ParseTupleAndKeywords(args, kwds, "sIII|dpp",
                                     const_cast<char **>(kwlist), &dll,
                                     &vendor_id, &device_id, &generated,
                                     &flow_scale, &performance, &fp16))
        return nullptr;
    if (g_initialized) {
        PyErr_SetString(g_error, "LSFG is already initialized; call finalize()");
        return nullptr;
    }
    if (generated < 1 || generated > 4) {
        PyErr_SetString(PyExc_ValueError, "generated must be 1 to 4");
        return nullptr;
    }

    auto set = std::make_shared<tlsfg::ShaderSet>();
    const int32_t rc = extract(dll, *set);
    if (rc != tlsfg::FgResult::OK) {
        PyErr_Format(g_error, "Cannot read the shaders from %s (%s)", dll,
                     shader_error(rc));
        return nullptr;
    }

    const uint64_t uuid = static_cast<uint64_t>(vendor_id) << 32 | device_id;
    // LSFG takes the inverse of the user's flow scale (as Temple does)
    const float scale = 1.0f / std::clamp(static_cast<float>(flow_scale), 0.25f, 1.0f);
    const bool use_fp16 = fp16 && set->hasFp16();
    auto loader = tlsfg::makeLoader(set, use_fp16);
    const bool perf = performance != 0;
    if (!call([&] {
            if (perf)
                LSFG_3_1P::initialize(uuid, false, scale, generated, loader);
            else
                LSFG_3_1::initialize(uuid, false, scale, generated, loader);
        }))
        return nullptr;
    g_performance = perf;
    g_initialized = true;
    return PyBool_FromLong(use_fp16);
}

PyObject *lsfg_create_context(PyObject *, PyObject *args) {
    int in0, in1;
    PyObject *py_out;
    unsigned int width, height;
    int format;
    if (!PyArg_ParseTuple(args, "iiOIIi", &in0, &in1, &py_out, &width, &height,
                          &format))
        return nullptr;
    if (!require_initialized()) return nullptr;
    std::vector<int> out;
    if (!read_fds(py_out, out)) return nullptr;
    const VkExtent2D extent{width, height};
    const VkFormat vk_format = static_cast<VkFormat>(format);
    int32_t id = 0;
    if (!call([&] {
            id = g_performance
                     ? LSFG_3_1P::createContext(in0, in1, out, extent, vk_format)
                     : LSFG_3_1::createContext(in0, in1, out, extent, vk_format);
        }))
        return nullptr;
    return PyLong_FromLong(id);
}

PyObject *lsfg_present(PyObject *, PyObject *args) {
    int id, in_sem;
    PyObject *py_out;
    if (!PyArg_ParseTuple(args, "iiO", &id, &in_sem, &py_out)) return nullptr;
    if (!require_initialized()) return nullptr;
    std::vector<int> out;
    if (!read_fds(py_out, out)) return nullptr;
    if (!call([&] {
            if (g_performance)
                LSFG_3_1P::presentContext(id, in_sem, out);
            else
                LSFG_3_1::presentContext(id, in_sem, out);
        }))
        return nullptr;
    Py_RETURN_NONE;
}

PyObject *lsfg_delete_context(PyObject *, PyObject *args) {
    int id;
    if (!PyArg_ParseTuple(args, "i", &id)) return nullptr;
    if (!require_initialized()) return nullptr;
    if (!call([&] {
            if (g_performance)
                LSFG_3_1P::deleteContext(id);
            else
                LSFG_3_1::deleteContext(id);
        }))
        return nullptr;
    Py_RETURN_NONE;
}

PyObject *lsfg_finalize(PyObject *, PyObject *) {
    if (!g_initialized) Py_RETURN_NONE;
    const bool perf = g_performance;
    g_initialized = false;
    if (!call([&] {
            if (perf)
                LSFG_3_1P::finalize();
            else
                LSFG_3_1::finalize();
        }))
        return nullptr;
    Py_RETURN_NONE;
}

PyMethodDef lsfg_methods[] = {
    {"probe", lsfg_probe, METH_VARARGS,
     "probe(dll) -> 'ok', 'ok_fp32' or an error code: read the shader sets "
     "from Lossless.dll without touching the GPU."},
    {"initialize", reinterpret_cast<PyCFunction>(lsfg_initialize),
     METH_VARARGS | METH_KEYWORDS,
     "initialize(dll, vendor_id, device_id, generated, flow_scale=1.0, "
     "performance=False, fp16=True) -> bool (FP16 shaders in use).\n"
     "Create LSFG's device on the GPU with these PCI ids; *generated* frames "
     "per real frame (1 to 4)."},
    {"create_context", lsfg_create_context, METH_VARARGS,
     "create_context(in0_fd, in1_fd, out_fds, width, height, vk_format) -> int"},
    {"present", lsfg_present, METH_VARARGS,
     "present(context, in_semaphore_fd, out_semaphore_fds): generate the "
     "frames between the previous input and this one."},
    {"delete_context", lsfg_delete_context, METH_VARARGS,
     "delete_context(context)"},
    {"finalize", lsfg_finalize, METH_NOARGS,
     "finalize(): destroy every context and LSFG's device."},
    {nullptr, nullptr, 0, nullptr}};

PyModuleDef lsfg_module = {PyModuleDef_HEAD_INIT, "lsfg",
                           "lsfg-vk frame generation core.", -1, lsfg_methods};

} // namespace

PyMODINIT_FUNC PyInit_lsfg(void) {
    PyObject *m = PyModule_Create(&lsfg_module);
    if (!m) return nullptr;
    g_error = PyErr_NewException("lsfg.LsfgError", PyExc_RuntimeError, nullptr);
    if (!g_error || PyModule_AddObjectRef(m, "LsfgError", g_error) < 0) {
        Py_DECREF(m);
        return nullptr;
    }
    return m;
}
