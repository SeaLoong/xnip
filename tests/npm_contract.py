"""Local private npm artifact, process, integrity and lifecycle contracts; never publish."""
from __future__ import annotations
import ctypes
import importlib.util
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("npm_prepare", ROOT/"npm/prepare.py")
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
NODE = shutil.which("node")
BINARY = Path(os.environ.get("XNIP_TEST_BINARY", ROOT/"target/debug"/("xnip.exe" if os.name == "nt" else "xnip")))

class Rpc:
    def __init__(self, launcher, cwd, env):
        self.child = subprocess.Popen([NODE,str(launcher),"mcp","--receipts","text"],
            cwd=cwd,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
            text=True,encoding="utf-8",bufsize=1)
        self.queue = queue.Queue()
        def read():
            for line in self.child.stdout:
                try:self.queue.put(json.loads(line))
                except Exception as e:self.queue.put(e)
            self.queue.put(EOFError("MCP stdout ended"))
        self.reader = threading.Thread(target=read,daemon=True);self.reader.start()
        self.next = 0
        self.request("initialize",{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"npm-contract","version":"1"}})
        self.send({"jsonrpc":"2.0","method":"notifications/initialized"})
    def send(self,message):
        self.child.stdin.write(json.dumps(message,ensure_ascii=False)+"\n");self.child.stdin.flush()
    def request(self,method,params):
        self.next += 1
        self.send({"jsonrpc":"2.0","id":self.next,"method":method,"params":params})
        answer = self.queue.get(timeout=15)
        if isinstance(answer,BaseException):raise answer
        assert answer["id"]==self.next,answer
        return answer
    def close(self):
        if self.child.stdin is not None:
            try:self.child.stdin.close()
            except (BrokenPipeError,OSError):pass
        try:self.child.wait(timeout=15)
        except subprocess.TimeoutExpired:
            self.child.kill();self.child.wait();raise
        self.reader.join(timeout=5)
        errors=self.child.stderr.read()
        for stream in [self.child.stdout,self.child.stderr]:stream.close()
        return errors

class NpmContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not NODE or not BINARY.is_file():raise unittest.SkipTest("build the xnip binary and install Node/npm first")
        (ROOT/"target").mkdir(exist_ok=True)
        cls.temporary=tempfile.TemporaryDirectory(prefix="npm-contract-",dir=ROOT/"target")
        cls.base=Path(cls.temporary.name).resolve()
        assert cls.base.is_relative_to((ROOT/"target").resolve())
        cls.report=prepare.prepare(None,cls.base/"artifacts",True,BINARY.resolve())
        parent=str(Path(cls.report["output_directory"])/next(r["filename"] for r in cls.report["tarballs"] if r["name"]==prepare.PARENT_NAME))
        cls.install=cls.base/"installed"
        subprocess.run([*prepare.npm_command(),"install","--prefix",str(cls.install),"--cache",str(cls.base/"npm-cache"),
            "--offline","--ignore-scripts","--no-audit","--no-fund",parent],check=True,capture_output=True,timeout=90)
        cls.package=cls.install/"node_modules"/prepare.PARENT_NAME
    @classmethod
    def tearDownClass(cls):
        if hasattr(cls,"temporary"):
            assert cls.base.is_relative_to((ROOT/"target").resolve())
            cls.temporary.cleanup()
    def setUp(self):
        self.work=Path(tempfile.mkdtemp(prefix="case-",dir=self.base))
        self.env={**os.environ,"XNIP_NPM_CACHE_DIR":str(self.work/"exec-cache"),
            "XNIP_UPDATE_STATE_DIR":str(self.work/"update-state"),"XNIP_AUTO_UPDATE":"1"}
        self.launcher=self.package/"bin/xnip.cjs"
    def run_cli(self,*args,input=None,launcher=None,env=None):
        return subprocess.run([NODE,str(launcher or self.launcher),*args],cwd=self.work,env=env or self.env,
            input=input,capture_output=True,timeout=20)
    def clone(self):
        prefix=self.work/"installed copy"
        shutil.copytree(self.install,prefix)
        return prefix,prefix/"node_modules"/prepare.PARENT_NAME
    def host_entry(self,package=None):
        package=package or self.package
        manifest=json.loads((package/"native-manifest.json").read_text(encoding="utf-8"))
        return next(iter(manifest["platforms"].values()))
    def cached_executable(self):
        return next((self.work/"exec-cache").glob("*/"+("xnip.exe" if os.name=="nt" else "xnip")))
    def test_private_host_tarball_installs_offline_without_scripts(self):
        p=json.loads((self.package/"package.json").read_text(encoding="utf-8"))
        self.assertTrue(p["private"]);self.assertEqual(p["version"],self.report["version"])
        self.assertEqual(p["name"],prepare.PARENT_NAME)
        self.assertEqual(p["bin"],{"xnip":"./bin/xnip.cjs"})
        manifest=json.loads((self.package/"native-manifest.json").read_text(encoding="utf-8"))
        self.assertTrue(manifest["localOnly"]);self.assertEqual(len(manifest["platforms"]),1)
        result=self.run_cli("--version")
        self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(result.stdout.decode().strip(),"xnip "+p["version"])
        self.assertEqual(result.stderr,b"");self.assertFalse((self.work/"update-state").exists())
        self.assertNotIn(str(self.package),str(self.cached_executable()))

    def test_all_private_host_package_metadata_has_one_repository_and_prepack_gate(self):
        parent=json.loads((self.package/"package.json").read_text(encoding="utf8"))
        self.assertEqual(parent["repository"],prepare.REPOSITORY)
        entry=self.host_entry()
        native=self.package/"node_modules"/entry["package"]
        metadata=json.loads((native/"package.json").read_text(encoding="utf8"))
        self.assertEqual(metadata["repository"],prepare.REPOSITORY)
        self.assertIs(metadata["private"],True)
        for directory in [self.package,native]:
            result=subprocess.run([NODE,str(directory/"lib/validate.cjs")],capture_output=True,timeout=15)
            self.assertEqual(result.returncode,0,result.stderr)

    def test_private_host_parent_and_payload_cannot_become_public_by_flag_only(self):
        _,package=self.clone()
        native=package/"node_modules"/self.host_entry(package)["package"]
        for directory in [package,native]:
            file=directory/"package.json"
            metadata=json.loads(file.read_text(encoding="utf8"));metadata["private"]=False
            file.write_text(json.dumps(metadata),encoding="utf8")
            result=subprocess.run([NODE,str(directory/"lib/validate.cjs")],capture_output=True,timeout=15)
            self.assertNotEqual(result.returncode,0)
            self.assertIn(b"private flag",result.stderr)

    def test_native_prepack_rechecks_actual_binary_after_install(self):
        _,package=self.clone()
        entry=self.host_entry(package)
        native=package/"node_modules"/entry["package"]
        data=(native/entry["binary"]).read_bytes()
        (native/entry["binary"]).write_bytes(data[:-1]+bytes([data[-1]^1]))
        result=subprocess.run([NODE,str(native/"lib/validate.cjs")],capture_output=True,timeout=15)
        self.assertNotEqual(result.returncode,0)
        self.assertIn(b"checksum",result.stderr)

    def test_cli_bytes_and_native_exit_codes_survive_wrapper(self):
        result=self.run_cli("edit","空 格.txt","--write","--text-stdin",input="首\r\nvalue=old\r\n末".encode())
        self.assertEqual(result.returncode,0,result.stderr)
        result=self.run_cli("edit","空 格.txt","--old","old","--text","new")
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual((self.work/"空 格.txt").read_bytes(),"首\r\nvalue=new\r\n末".encode())
        result=self.run_cli("edit","空 格.txt","--old","absent","--text","bad")
        self.assertEqual(result.returncode,3);self.assertEqual(result.stdout,b"")
        self.assertFalse((self.work/"update-state").exists())
    def test_concurrent_first_launches_publish_one_immutable_cache_entry(self):
        children=[subprocess.Popen([NODE,str(self.launcher),"--version"],cwd=self.work,env=self.env,
            stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE) for _ in range(5)]
        for child in children:
            out,err=child.communicate(timeout=30);self.assertEqual(child.returncode,0,err);self.assertIn(b"xnip ",out)
        directories=[p for p in (self.work/"exec-cache").iterdir() if p.is_dir()]
        self.assertEqual(len(directories),1);self.assertFalse(directories[0].name.startswith(".prepare-"))
    def test_tampered_cached_binary_is_rejected_without_repair(self):
        self.assertEqual(self.run_cli("--version").returncode,0)
        binary=self.cached_executable();body=binary.read_bytes();binary.write_bytes(body[:-1]+bytes([body[-1]^1]))
        changed=prepare.digest(binary.read_bytes())
        result=self.run_cli("--version");self.assertNotEqual(result.returncode,0);self.assertEqual(result.stdout,b"")
        self.assertIn(b"integrity",result.stderr);self.assertEqual(prepare.digest(binary.read_bytes()),changed)
    def test_payload_tampering_and_missing_optional_package_are_explicit(self):
        _,package=self.clone();entry=self.host_entry(package)
        payload=package/"node_modules"/entry["package"]/entry["binary"]
        payload.write_bytes(b"not the verified executable")
        result=self.run_cli("--version",launcher=package/"bin/xnip.cjs")
        self.assertNotEqual(result.returncode,0);self.assertIn(b"integrity",result.stderr)
        payload.parent.rename(payload.parent.with_name("unavailable-platform"))
        result=self.run_cli("--version",launcher=package/"bin/xnip.cjs")
        self.assertNotEqual(result.returncode,0);self.assertIn(b"missing optional platform",result.stderr)
    def test_parent_metadata_version_and_architecture_validation(self):
        _,package=self.clone();file=package/"native-manifest.json"
        data=json.loads(file.read_text(encoding="utf-8"));data["version"]="../../untrusted"
        file.write_text(json.dumps(data),encoding="utf-8")
        r=self.run_cli("--version",launcher=package/"bin/xnip.cjs")
        self.assertNotEqual(r.returncode,0);self.assertIn(b"version mismatch",r.stderr)
        module=self.package/"lib/native.cjs"
        js="const n=require(process.argv[1]);for(const p of ['linux','darwin','win32'])for(const a of ['x64','arm64'])console.log(n.platformKey(p,a));try{n.platformKey('linux','ia32');process.exit(2)}catch{}"
        r=subprocess.run([NODE,"-e",js,str(module)],capture_output=True,timeout=10)
        self.assertEqual(r.returncode,0,r.stderr);self.assertEqual(len(r.stdout.splitlines()),6)
    def test_clean_mcp_stream_and_eof_shutdown(self):
        rpc=Rpc(self.launcher,self.work,self.env)
        try:
            tools=rpc.request("tools/list",{})["result"]["tools"];self.assertEqual(len(tools),5)
            for tool in tools:
                if tool["name"] in ["xnip_edit","xnip_apply","xnip_recover"]:self.assertNotIn("outputSchema",tool)
            r=rpc.request("tools/call",{"name":"xnip_apply","arguments":{"files":{"a.txt":"before\r\nEOF"}}})
            self.assertNotIn("error",r);self.assertEqual((self.work/"a.txt").read_bytes(),b"before\r\nEOF")
            r=rpc.request("tools/call",{"name":"xnip_edit","arguments":{"file":"a.txt","old":"before","text":"after"}})
            self.assertNotIn("structuredContent",r["result"]);self.assertNotIn("isError",r["result"])
            self.assertEqual(r["result"]["content"][0]["text"],"written")
        finally:self.assertEqual(rpc.close(),"")
        self.assertEqual(rpc.child.returncode,0);self.assertFalse((self.work/"update-state").exists())
    def test_package_uninstall_does_not_interrupt_mapped_mcp(self):
        prefix,package=self.clone();rpc=Rpc(package/"bin/xnip.cjs",self.work,self.env)
        try:
            subprocess.run([*prepare.npm_command(),"uninstall",prepare.PARENT_NAME,"--prefix",str(prefix),
                "--offline","--ignore-scripts","--no-audit","--no-fund"],check=True,capture_output=True,timeout=30)
            self.assertFalse(package.exists())
            r=rpc.request("tools/call",{"name":"xnip_edit","arguments":{"file":"still-live","operation":"write","text":"ok"}})
            self.assertNotIn("error",r);self.assertEqual((self.work/"still-live").read_bytes(),b"ok")
        finally:self.assertEqual(rpc.close(),"")
    def test_abrupt_wrapper_death_closes_native_mcp_input(self):
        rpc=Rpc(self.launcher,self.work,self.env)
        if os.name=="nt":
            result=subprocess.check_output(["powershell","-NoProfile","-Command",
                f"""Get-CimInstance Win32_Process -Filter "ParentProcessId={rpc.child.pid} AND Name='xnip.exe'" | Select-Object -ExpandProperty ProcessId"""],text=True)
            pid=int(result.strip())
            kernel=ctypes.WinDLL("kernel32",use_last_error=True)
            kernel.OpenProcess.argtypes=[ctypes.c_uint32,ctypes.c_int,ctypes.c_uint32];kernel.OpenProcess.restype=ctypes.c_void_p
            kernel.WaitForSingleObject.argtypes=[ctypes.c_void_p,ctypes.c_uint32];kernel.WaitForSingleObject.restype=ctypes.c_uint32
            kernel.CloseHandle.argtypes=[ctypes.c_void_p]
            handle=kernel.OpenProcess(0x100000,False,pid);self.assertTrue(handle)
            try:
                rpc.child.kill();rpc.child.wait(timeout=10)
                # Keep the caller's stdin writer open while observing child EOF.
                self.assertEqual(kernel.WaitForSingleObject(handle,10000),0)
            finally:kernel.CloseHandle(handle);rpc.close()
        else:
            try:
                rpc.child.terminate();rpc.child.wait(timeout=10)
            finally:rpc.close()

if __name__=="__main__":unittest.main()
