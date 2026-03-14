#!/usr/bin/env bun
/**
 * Restart EchoType dev server.
 *
 * Usage:
 *   bun run restart
 *
 * Kills any running EchoType/tauri processes, then launches `bun run dev`
 * in the background. Works on Windows, macOS, and Linux.
 */

import { execSync, spawn } from "child_process";
import { resolve, join } from "path";
import { writeFileSync, unlinkSync, existsSync } from "fs";
import { homedir, platform } from "os";

const PROJECT_DIR = resolve(import.meta.dirname, "..");
const os = platform();

// ── Kill existing processes ─────────────────────────────────────────────

function kill() {
  if (os === "win32") {
    // Kill echotype.exe first (the Tauri app)
    try {
      execSync('taskkill /F /IM echotype.exe', { stdio: "ignore" });
      console.log("Stopped echotype.exe");
    } catch { /* not running */ }

    // Find and kill the bun process tree running tauri dev.
    // Look for bun processes whose command line contains "tauri dev".
    try {
      const wmic = execSync(
        'wmic process where "name=\'bun.exe\'" get ProcessId,CommandLine /FORMAT:CSV',
        { encoding: "utf-8", stdio: ["pipe", "pipe", "pipe"] }
      );
      for (const line of wmic.split("\n")) {
        if (line.includes("tauri") && line.includes("dev")) {
          const pid = line.trim().split(",").pop()?.trim();
          if (pid && /^\d+$/.test(pid)) {
            try {
              execSync(`taskkill /F /T /PID ${pid}`, { stdio: "ignore" });
              console.log(`Stopped bun process tree (PID ${pid})`);
            } catch { /* already gone */ }
          }
        }
      }
    } catch { /* no matching processes */ }
  } else {
    // macOS / Linux: pkill by name
    try { execSync("pkill -f 'echotype'", { stdio: "ignore" }); } catch {}
    try { execSync("pkill -f 'tauri dev'", { stdio: "ignore" }); } catch {}
    console.log("Stopped existing processes");
  }
}

// ── Launch ──────────────────────────────────────────────────────────────

function launch() {
  console.log(`\nStarting EchoType from ${PROJECT_DIR}...\n`);

  if (os === "win32") {
    // Use WScript.Shell to launch without a visible console window
    const bunPath = join(homedir(), ".bun", "bin", "bun.exe").replace(/\//g, "\\");
    const cargoDir = join(homedir(), ".cargo", "bin").replace(/\//g, "\\");
    const bunDir = join(homedir(), ".bun", "bin").replace(/\//g, "\\");
    const projectPath = PROJECT_DIR.replace(/\//g, "\\");

    const vbs = join(PROJECT_DIR, "scripts", "restart-hidden.vbs");
    writeFileSync(vbs, `Set WshShell = CreateObject("WScript.Shell")
Set WshEnv = WshShell.Environment("Process")
WshEnv("PATH") = "${cargoDir};" & "${bunDir}" & ";" & WshEnv("PATH")
WshShell.CurrentDirectory = "${projectPath}"
WshShell.Run """${bunPath}"" run tauri dev", 0, False
`);
    // Run the VBS launcher then clean it up
    execSync(`cscript //nologo "${vbs}"`, { stdio: "ignore" });
    // Small delay before cleanup so WScript has time to spawn the child
    setTimeout(() => {
      try { unlinkSync(vbs); } catch {}
    }, 2000);
  } else {
    const child = spawn("bash", ["-c", "source .env && bun run tauri dev"], {
      cwd: PROJECT_DIR,
      stdio: "ignore",
      detached: true,
    });
    child.unref();
  }

  console.log("EchoType is starting in the background.");
  console.log("Check logs at: %APPDATA%/com.echotype.app/logs/ (Windows)");
  console.log("              ~/Library/Application Support/com.echotype.app/logs/ (macOS)");
  console.log("              $XDG_DATA_HOME/com.echotype.app/logs/ (Linux)");
}

// ── Clean stale tray icons (Windows) ────────────────────────────────────

function cleanStaleTrayIcons() {
  if (os !== "win32") return;

  // Sweep the notification area with simulated mouse moves.
  // This forces Windows to re-check which tray icons are still alive
  // and removes any that belong to dead processes.
  const ps = `
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class TrayRefresh {
    [DllImport("user32.dll")] static extern IntPtr FindWindow(string cls, string win);
    [DllImport("user32.dll")] static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string win);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr w, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
    public static void Refresh() {
        IntPtr tray = FindWindow("Shell_TrayWnd", null);
        IntPtr nota = FindWindowEx(tray, IntPtr.Zero, "TrayNotifyWnd", null);
        IntPtr pager = FindWindowEx(nota, IntPtr.Zero, "SysPager", null);
        IntPtr toolbar = FindWindowEx(pager, IntPtr.Zero, "ToolbarWindow32", null);
        if (toolbar == IntPtr.Zero) toolbar = FindWindowEx(nota, IntPtr.Zero, "ToolbarWindow32", null);
        if (toolbar == IntPtr.Zero) return;
        RECT r; GetClientRect(toolbar, out r);
        for (int x = 0; x < r.R; x += 8)
            for (int y = 0; y < r.B; y += 8)
                SendMessage(toolbar, 0x0200, IntPtr.Zero, (IntPtr)((y << 16) | x));
    }
}
"@
[TrayRefresh]::Refresh()
`;
  try {
    execSync(`powershell -NoProfile -Command "${ps.replace(/\n/g, " ")}"`, {
      stdio: "ignore",
      timeout: 5000,
    });
  } catch { /* best effort */ }
}

// ── Main ────────────────────────────────────────────────────────────────

console.log("Restarting EchoType...\n");
kill();

// Brief pause to let processes fully exit, then clean stale tray icons
await Bun.sleep(1000);
cleanStaleTrayIcons();

launch();
