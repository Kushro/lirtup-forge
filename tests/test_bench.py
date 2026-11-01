"""upscaler.bench with a simulated GPU: measuring, cancelling, storing, CLI."""

import json
import threading
from datetime import datetime, timezone

import pytest

from upscaler import bench
from upscaler.bench import (
    BenchCase,
    BenchResult,
    Benchmark,
    BenchSettings,
    FRAMEGEN,
    MAGPIEFX,
    NATIVE,
)


class FakeRunner:
    def __init__(self, times, fail_at=None, on_step=None, extra=None):
        self.times = list(times)
        self.fail_at = fail_at
        self.on_step = on_step
        self.steps = 0
        self.closed = False
        if extra is not None:
            self.extra = extra

    def step(self):
        i = self.steps
        self.steps += 1
        if self.on_step is not None:
            self.on_step(i)
        if self.fail_at is not None and i == self.fail_at:
            raise RuntimeError("device lost\nmore detail")
        return self.times[i % len(self.times)]

    def close(self):
        self.closed = True


class FakeBackend:
    gpu_name = "Fake GPU"
    gpu_key = "1002:7550:Fake GPU"

    def __init__(self, runners=None, errors=None):
        self.runners = runners or {}
        self.errors = errors or {}
        self.created = []
        self.closed = False

    def runner(self, case):
        self.created.append(case.key)
        if case.name in self.errors:
            raise self.errors[case.name]
        runner = self.runners.get(case.name)
        if runner is None:
            runner = self.runners[case.name] = FakeRunner([1.0])
        return runner

    def close(self):
        self.closed = True


def settings(**kw):
    base = dict(warmup=2, min_frames=3, max_frames=5, budget_s=100.0)
    base.update(kw)
    return BenchSettings(**base)


# ----------------------------------------------------------------------
# Measuring
# ----------------------------------------------------------------------
def test_measure_skips_warmup_and_reports_median_and_p95():
    # Two warm-up frames (100 ms) are not counted
    runner = FakeRunner([100.0, 100.0, 1.0, 3.0, 2.0, 5.0, 4.0])
    backend = FakeBackend({"LumaSharpen": runner})
    result = Benchmark(backend, settings()).measure(BenchCase(NATIVE, "LumaSharpen"))
    assert result.ok
    assert result.frames == 5
    assert result.ms == 3.0
    assert result.p95 == 5.0
    assert runner.steps == 7
    assert runner.closed


def test_time_budget_stops_after_min_frames():
    clock_values = iter([0.0] + [10.0] * 100)
    runner = FakeRunner([2.0])
    bench_ = Benchmark(
        FakeBackend({"x": runner}),
        settings(min_frames=3, max_frames=50, budget_s=1.0),
        clock=lambda: next(clock_values),
    )
    result = bench_.measure(BenchCase(MAGPIEFX, "x"))
    assert result.frames == 3


def test_failing_step_is_reported_and_runner_closed():
    runner = FakeRunner([1.0], fail_at=3)
    result = Benchmark(FakeBackend({"x": runner}), settings()).measure(
        BenchCase(MAGPIEFX, "x")
    )
    assert not result.ok
    assert result.error == "device lost"
    assert runner.closed


def test_failing_runner_creation_is_reported():
    backend = FakeBackend(errors={"x": ValueError("cannot compile")})
    result = Benchmark(backend, settings()).measure(BenchCase(MAGPIEFX, "x"))
    assert result.error == "cannot compile"
    assert result.ms is None


def test_runner_extra_is_kept():
    runner = FakeRunner([1.0], extra={"fp16": True})
    result = Benchmark(FakeBackend({"x2": runner}), settings()).measure(
        BenchCase(FRAMEGEN, "x2", 2)
    )
    assert result.extra == {"fp16": True}


def test_run_goes_on_after_a_failure():
    backend = FakeBackend(errors={"b": RuntimeError("boom")})
    cases = [BenchCase(MAGPIEFX, n) for n in ("a", "b", "c")]
    seen = []
    results = Benchmark(backend, settings()).run(
        cases, on_result=lambda done, total, r: seen.append((done, total, r.name))
    )
    assert [r.name for r in results] == ["a", "b", "c"]
    assert [r.ok for r in results] == [True, False, True]
    assert seen == [(1, 3, "a"), (2, 3, "b"), (3, 3, "c")]


def test_cancel_between_cases_keeps_finished_results():
    backend = FakeBackend()
    b = Benchmark(backend, settings())
    cases = [BenchCase(MAGPIEFX, n) for n in ("a", "b", "c")]
    results = b.run(cases, on_result=lambda *_: b.cancel())
    assert [r.name for r in results] == ["a"]
    assert b.cancelled
    assert backend.created == ["magpiefx:a"]


def test_cancel_during_a_case_closes_its_runner():
    b = None

    def on_step(i):
        if i == 4:
            b.cancel()

    runner = FakeRunner([1.0], on_step=on_step)
    backend = FakeBackend({"a": runner})
    b = Benchmark(backend, settings(max_frames=1000))
    results = b.run([BenchCase(MAGPIEFX, "a"), BenchCase(MAGPIEFX, "b")])
    assert results == []
    assert runner.closed
    assert "magpiefx:b" not in backend.created


def test_cancel_from_another_thread():
    started = threading.Event()
    b = None

    def on_step(i):
        started.set()

    runner = FakeRunner([1.0], on_step=on_step)
    b = Benchmark(FakeBackend({"a": runner}), settings(max_frames=10**9, budget_s=10**9))
    t = threading.Thread(target=lambda: (started.wait(5), b.cancel()))
    t.start()
    results = b.run([BenchCase(MAGPIEFX, "a")])
    t.join(5)
    assert results == []
    assert runner.closed


def test_summarize():
    assert bench.summarize([3.0]) == (3.0, 3.0)
    median, p95 = bench.summarize([float(i) for i in range(1, 101)])
    assert median == 50.5
    assert p95 == 95.0


# ----------------------------------------------------------------------
# Cases
# ----------------------------------------------------------------------
def test_effect_cases_start_with_the_native_engine():
    cases = bench.build_cases([bench.SECTION_EFFECTS], ["CAS", "FSR_EASU"])
    assert [(c.kind, c.name) for c in cases] == [
        (NATIVE, "LumaSharpen"),
        (NATIVE, "Anime4K_Upscale_S"),
        (NATIVE, "CRT_Easymode"),
        (MAGPIEFX, "CAS"),
        (MAGPIEFX, "FSR_EASU"),
    ]


def test_framegen_cases_cover_every_setting():
    cases = bench.build_cases([bench.SECTION_FRAMEGEN])
    combos = {(c.multiplier, c.flow_scale, c.performance) for c in cases}
    assert combos == {
        (m, f, p) for m in (2, 3, 4) for f in (1.0, 0.5) for p in (False, True)
    }
    assert len({c.name for c in cases}) == 12
    assert all(c.kind == FRAMEGEN for c in cases)


def test_framegen_key():
    assert bench.framegen_key(2, 1.0, False) == "x2 flow 1.0 quality"
    assert bench.framegen_key(4, 0.5, True) == "x4 flow 0.5 performance"


def test_gpu_key():
    assert bench.gpu_key("AMD Radeon RX 9070 XT", 0x1002, 0x7550) == (
        "1002:7550:AMD Radeon RX 9070 XT"
    )


# ----------------------------------------------------------------------
# Stored results
# ----------------------------------------------------------------------
GPU_A = {"key": "1002:7550:Radeon A", "name": "Radeon A"}
GPU_B = {"key": "1002:164e:Radeon B", "name": "Radeon B"}
NOW = datetime(2026, 11, 1, tzinfo=timezone.utc)


def test_results_path_is_next_to_the_config(isolated_home):
    assert bench.results_path() == str(
        isolated_home / ".config/linux-rt-upscaler/bench.json"
    )


def test_save_and_load_round_trip(tmp_path):
    path = str(tmp_path / "bench.json")
    results = [
        BenchResult(NATIVE, "LumaSharpen", 0.0161234, 0.02, 100),
        BenchResult(MAGPIEFX, "Broken", error="not found"),
    ]
    bench.save_results(GPU_A, results, (1280, 720), (2560, 1440), path=path, now=NOW)
    entry = bench.results_for_gpu(None, bench.load_results(path))
    assert entry["name"] == "Radeon A"
    assert bench.stored_ms(entry, NATIVE, "LumaSharpen") == pytest.approx(0.0161)
    assert entry[MAGPIEFX]["Broken"]["error"] == "not found"
    assert bench.stored_ms(entry, MAGPIEFX, "Broken") is None
    assert entry["sizes"][NATIVE] == {"input": [1280, 720], "target": [2560, 1440]}
    assert entry["updated"][NATIVE].startswith("2026-11-01")


def test_new_results_merge_and_replace(tmp_path):
    path = str(tmp_path / "bench.json")
    size = ((1280, 720), (2560, 1440))
    bench.save_results(GPU_A, [BenchResult(MAGPIEFX, "A", 1.0), BenchResult(MAGPIEFX, "B", 2.0)], *size, path=path)
    bench.save_results(GPU_A, [BenchResult(MAGPIEFX, "B", 3.0)], *size, path=path)
    entry = bench.load_results(path)["gpus"][GPU_A["key"]]
    assert bench.stored_ms(entry, MAGPIEFX, "A") == 1.0
    assert bench.stored_ms(entry, MAGPIEFX, "B") == 3.0


def test_other_sizes_replace_the_kind(tmp_path):
    path = str(tmp_path / "bench.json")
    bench.save_results(GPU_A, [BenchResult(MAGPIEFX, "A", 1.0), BenchResult(NATIVE, "N", 1.0)], (1280, 720), (2560, 1440), path=path)
    bench.save_results(GPU_A, [BenchResult(MAGPIEFX, "B", 2.0)], (1920, 1080), (3840, 2160), path=path)
    entry = bench.load_results(path)["gpus"][GPU_A["key"]]
    assert bench.stored_ms(entry, MAGPIEFX, "A") is None
    assert bench.stored_ms(entry, MAGPIEFX, "B") == 2.0
    # Other kinds keep their own sizes
    assert bench.stored_ms(entry, NATIVE, "N") == 1.0


def test_framegen_sizes_only_have_the_target(tmp_path):
    path = str(tmp_path / "bench.json")
    key = bench.framegen_key(2, 1.0, False)
    bench.save_results(GPU_A, [BenchResult(FRAMEGEN, key, 4.0)], (1280, 720), (2560, 1440), path=path)
    entry = bench.load_results(path)["gpus"][GPU_A["key"]]
    assert entry["sizes"][FRAMEGEN] == {"target": [2560, 1440]}
    assert bench.stored_framegen_ms(entry, 2, 1.0, False) == 4.0
    assert bench.stored_framegen_ms(entry, 3, 1.0, False) is None


def test_results_for_gpu_follows_the_selector(tmp_path):
    path = str(tmp_path / "bench.json")
    size = ((1280, 720), (2560, 1440))
    bench.save_results(GPU_A, [BenchResult(NATIVE, "N", 1.0)], *size, selector=None, path=path)
    bench.save_results(GPU_B, [BenchResult(NATIVE, "N", 2.0)], *size, selector="1", path=path)
    data = bench.load_results(path)
    assert bench.results_for_gpu(None, data)["name"] == "Radeon A"
    assert bench.results_for_gpu("1", data)["name"] == "Radeon B"
    # Not measured with that selector: match by name
    assert bench.results_for_gpu("radeon b", data)["name"] == "Radeon B"
    # Ambiguous: nothing rather than another GPU's times
    assert bench.results_for_gpu("2", data) is None


def test_results_for_gpu_with_one_gpu(tmp_path):
    path = str(tmp_path / "bench.json")
    bench.save_results(GPU_A, [BenchResult(NATIVE, "N", 1.0)], (1, 1), (1, 1), selector="0", path=path)
    assert bench.results_for_gpu("anything", bench.load_results(path))["name"] == "Radeon A"


def test_missing_or_corrupt_file_has_no_results(tmp_path):
    path = tmp_path / "bench.json"
    assert bench.results_for_gpu(None, bench.load_results(str(path))) is None
    path.write_text("{not json")
    assert bench.load_results(str(path))["gpus"] == {}
    path.write_text('["a list"]')
    assert bench.load_results(str(path))["gpus"] == {}
    assert bench.stored_ms(None, NATIVE, "x") is None


# ----------------------------------------------------------------------
# Command line
# ----------------------------------------------------------------------
def fake_factory(backend):
    calls = []

    def factory(input_size, target_size, gpu, dll):
        calls.append((input_size, target_size, gpu, dll))
        return backend

    factory.calls = calls
    return factory


def events(capsys):
    return [json.loads(line) for line in capsys.readouterr().out.splitlines() if line]


def test_cli_emits_events_and_saves(capsys):
    backend = FakeBackend()
    factory = fake_factory(backend)
    rc = bench.main(
        ["--json", "--save", "--section", "effects", "--effect", "CAS",
         "--input-size", "640x480", "--target-size", "1280x960", "--max-frames", "3",
         "--gpu", "0"],
        backend_factory=factory,
    )
    assert rc == 0
    assert factory.calls == [((640, 480), (1280, 960), "0", None)]
    ev = events(capsys)
    assert ev[0]["event"] == "start"
    assert ev[0]["total"] == 4
    assert ev[0]["gpu"] == {"key": backend.gpu_key, "name": backend.gpu_name}
    results = [e for e in ev if e["event"] == "result"]
    assert [r["name"] for r in results] == ["LumaSharpen", "Anime4K_Upscale_S", "CRT_Easymode", "CAS"]
    assert results[-1]["done"] == 4 and results[-1]["ms"] == 1.0
    assert ev[-1] == {"event": "done", "count": 4}
    assert backend.closed

    entry = bench.results_for_gpu("0")
    assert bench.stored_ms(entry, MAGPIEFX, "CAS") == 1.0
    assert entry["sizes"][MAGPIEFX] == {"input": [640, 480], "target": [1280, 960]}


def test_cli_framegen_without_dll_reports_each_case(capsys):
    backend = FakeBackend(errors={c.name: RuntimeError("Lossless.dll not found") for c in bench.framegen_cases()})
    rc = bench.main(["--json", "--section", "framegen"], backend_factory=fake_factory(backend))
    assert rc == 0
    results = [e for e in events(capsys) if e["event"] == "result"]
    assert len(results) == 12
    assert all(r["error"] == "Lossless.dll not found" for r in results)
    # Not saved without --save
    assert bench.results_for_gpu(None) is None


def test_cli_backend_failure(capsys):
    def factory(*args):
        raise RuntimeError("no Vulkan device")

    assert bench.main(["--json"], backend_factory=factory) == 1
    assert events(capsys) == [{"event": "error", "error": "no Vulkan device"}]


def test_cli_rejects_bad_sizes():
    with pytest.raises(SystemExit):
        bench.main(["--input-size", "10x10"], backend_factory=fake_factory(FakeBackend()))
    with pytest.raises(SystemExit):
        bench.main(["--target-size", "wide"], backend_factory=fake_factory(FakeBackend()))
