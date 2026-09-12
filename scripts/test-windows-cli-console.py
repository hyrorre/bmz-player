"""Windows release CLI console regression checks (standard library only).

Usage: python scripts/test-windows-cli-console.py target/release/bmz-player.exe
Creates a hidden console host; never opens a visible console or game window.
"""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile


def host(executable, report):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetStdHandle.argtypes = [wintypes.DWORD]
    kernel.GetStdHandle.restype = wintypes.HANDLE
    kernel.GetConsoleWindow.restype = wintypes.HWND

    class Coord(ctypes.Structure):
        _fields_ = [("x", ctypes.c_short), ("y", ctypes.c_short)]

    kernel.ReadConsoleOutputCharacterW.argtypes = [
        wintypes.HANDLE, wintypes.LPWSTR, wintypes.DWORD, Coord,
        ctypes.POINTER(wintypes.DWORD),
    ]
    kernel.FillConsoleOutputCharacterW.argtypes = [
        wintypes.HANDLE, wintypes.WCHAR, wintypes.DWORD, Coord,
        ctypes.POINTER(wintypes.DWORD),
    ]
    kernel.SetConsoleCursorPosition.argtypes = [wintypes.HANDLE, Coord]
    console = kernel.GetStdHandle(-11 & 0xFFFFFFFF)
    assert kernel.GetConsoleWindow(), "hidden test host must own a console"

    def clear():
        written = wintypes.DWORD()
        assert kernel.FillConsoleOutputCharacterW(console, " ", 32768, Coord(), ctypes.byref(written))
        assert kernel.SetConsoleCursorPosition(console, Coord())

    def text():
        buffer = ctypes.create_unicode_buffer(32769)
        count = wintypes.DWORD()
        assert kernel.ReadConsoleOutputCharacterW(console, buffer, 32768, Coord(), ctypes.byref(count))
        return buffer[:count.value]

    def invoke(args, stdout=0, stderr=0):
        # Explicitly empty standard handles model a GUI launcher. Mixed handles
        # exercise restoration after AttachConsole as well as plain inheritance.
        startup = subprocess.STARTUPINFO()
        startup.dwFlags = subprocess.STARTF_USESTDHANDLES
        startup.hStdInput = 0
        startup.hStdOutput = stdout
        startup.hStdError = stderr
        child = subprocess.Popen([str(executable), *args], startupinfo=startup, close_fds=False)
        return child.wait(timeout=30)

    clear()
    assert invoke(["--help"]) == 0
    assert "bmz-player" in text(), "help did not reach parent's console"
    clear()
    assert invoke(["--gauge", "invalid"]) == 1
    assert "invalid --gauge" in text(), "parse error did not reach parent's console"

    import msvcrt
    with tempfile.TemporaryDirectory(prefix="bmz-console-streams-") as directory:
        output = Path(directory) / "stdout.txt"
        with output.open("wb") as stream:
            handle = msvcrt.get_osfhandle(stream.fileno())
            os.set_handle_inheritable(handle, True)
            clear()
            assert invoke(["--help"], stdout=handle) == 0
            assert "bmz-player" not in text(), "stdout redirection was replaced"
        assert "bmz-player" in output.read_text(encoding="utf-8")
        error = Path(directory) / "stderr.txt"
        with error.open("wb") as stream:
            handle = msvcrt.get_osfhandle(stream.fileno())
            os.set_handle_inheritable(handle, True)
            clear()
            assert invoke(["--gauge", "invalid"], stderr=handle) == 1
            assert "invalid --gauge" not in text(), "stderr redirection was replaced"
        assert "invalid --gauge" in error.read_text(encoding="utf-8")

    captured = subprocess.run([str(executable), "--help"], capture_output=True, timeout=30)
    assert captured.returncode == 0 and b"bmz-player" in captured.stdout
    assert not captured.stderr
    clear()
    powershell = subprocess.run([
        "powershell.exe", "-NoProfile", "-NonInteractive", "-Command",
        "& '" + str(executable).replace("'", "''") + "' --help",
    ], timeout=30)
    assert powershell.returncode == 0 and "bmz-player" in text()
    clear()
    cmd = subprocess.run(f'"{executable}" --help', shell=True, timeout=30)
    assert cmd.returncode == 0 and "bmz-player" in text()
    report.write_text(json.dumps({"parent_console": True, "mixed_file_redirection": True,
                                 "pipe_redirection": True, "powershell": True,
                                 "cmd": True}), encoding="utf-8")


def detached(executable, report):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetConsoleWindow.restype = wintypes.HWND
    assert not kernel.GetConsoleWindow()
    startup = subprocess.STARTUPINFO()
    startup.dwFlags = subprocess.STARTF_USESTDHANDLES
    startup.hStdInput = startup.hStdOutput = startup.hStdError = 0
    for args, code in [(["--help"], 0), (["--gauge", "invalid"], 1)]:
        process = subprocess.Popen([str(executable), *args], startupinfo=startup, close_fds=False)
        assert process.wait(timeout=30) == code
    assert not kernel.GetConsoleWindow()
    report.write_text(json.dumps({"no_parent_console": True}), encoding="utf-8")


def main():
    if os.name != "nt":
        raise SystemExit("This test requires Windows")
    executable = Path(sys.argv[1]).resolve()
    if len(sys.argv) > 2 and sys.argv[2] in ("--host", "--detached"):
        try:
            (host if sys.argv[2] == "--host" else detached)(executable, Path(sys.argv[3]))
        except Exception:
            import traceback
            Path(sys.argv[3]).write_text(traceback.format_exc(), encoding="utf-8")
            raise
        return
    image = executable.read_bytes()
    pe = struct.unpack_from("<I", image, 0x3C)[0]
    assert image[pe:pe + 4] == b"PE\0\0"
    assert struct.unpack_from("<H", image, pe + 24 + 68)[0] == 2, "release must retain GUI subsystem"
    startup = subprocess.STARTUPINFO()
    startup.dwFlags = subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = subprocess.SW_HIDE
    with tempfile.TemporaryDirectory(prefix="bmz-console-check-") as directory:
        report = Path(directory) / "report.json"
        for mode, flags in [("--host", subprocess.CREATE_NEW_CONSOLE),
                            ("--detached", subprocess.DETACHED_PROCESS)]:
            report.unlink(missing_ok=True)
            helper = subprocess.Popen(
                [sys.executable, str(Path(__file__).resolve()), str(executable), mode, str(report)],
                creationflags=flags, startupinfo=startup,
            )
            try:
                status = helper.wait(timeout=120)
                result = report.read_text(encoding="utf-8") if report.exists() else "no report"
                assert status == 0, result
                print(result)
            finally:
                if helper.poll() is None:
                    helper.kill()
                    helper.wait()


if __name__ == "__main__":
    main()
