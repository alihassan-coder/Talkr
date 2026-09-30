// Builds the speech engine sidecars that Tauri bundles next to the app (bundle.externalBin):
//   src-tauri/binaries/talkr-engine-<triple>[.exe]      CPU build (on macOS: CPU + Metal)
//   src-tauri/binaries/talkr-engine-gpu-<triple>[.exe]  Vulkan build (Windows and Linux only)
//
// Runs as part of `tauri build` / `tauri dev` (beforeBuildCommand / beforeDevCommand), which set
// TAURI_ENV_TARGET_TRIPLE and TAURI_ENV_DEBUG. Can also be run directly:
//   node scripts/build-engine.mjs [--debug] [--target <triple>]
//
// Environment:
//   TALKR_REQUIRE_GPU=1  fail instead of falling back when the Vulkan build can't be made (CI).
//   TALKR_NATIVE=1       tune whisper.cpp for this machine's CPU (never for builds you share).
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { dirname, join, parse } from 'node:path'
import { fileURLToPath } from 'node:url'

const tauriDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'src-tauri')
const args = process.argv.slice(2)
const flag = (name) => args.includes(name)
const option = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined)

const hostTriple = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (\S+)$/m)[1]
const explicitTarget = option('--target') ?? process.env.TAURI_ENV_TARGET_TRIPLE
const triple = explicitTarget ?? hostTriple
const debug = flag('--debug') || process.env.TAURI_ENV_DEBUG === 'true'
const isWindows = triple.includes('windows')
const isMac = triple.includes('apple-darwin')
const exe = isWindows ? '.exe' : ''

// cargo puts cross builds under target/<triple>; `--target` for the host itself would move the
// sherpa-onnx libraries away from target/release, where the bundle configs expect them.
const crossTarget = explicitTarget && explicitTarget !== hostTriple ? explicitTarget : undefined
const targetDir = process.env.CARGO_TARGET_DIR ?? join(tauriDir, 'target')
const outDir = join(targetDir, ...(crossTarget ? [crossTarget] : []), debug ? 'debug' : 'release')
const binDir = join(tauriDir, 'binaries')
mkdirSync(binDir, { recursive: true })

function buildEnv() {
  const env = { ...process.env }
  // Optimization flags for whisper.cpp come from src-tauri/.cargo/config.toml, which every cargo
  // command (tests and CI included) picks up.
  // Tuning for the build machine's CPU makes the engine crash with "illegal instruction" on
  // other PCs. Apple Silicon is the exception: every M-series chip runs the native build.
  if (!env.GGML_NATIVE) {
    env.GGML_NATIVE = process.env.TALKR_NATIVE === '1' || triple === 'aarch64-apple-darwin' ? 'ON' : 'OFF'
  }
  return env
}

// The Vulkan backend configures a nested CMake project (its shader generator) deep inside
// target/, and MSBuild's file tracker fails past Windows' 260-character path limit. Build the
// Windows GPU engine in a short target folder instead.
const gpuTargetDir =
  isWindows && (process.env.CARGO_TARGET_DIR ?? targetDir).length > 24 ? join(parse(tauriDir).root, 'talkr-vk') : targetDir

function cargoBuild(features) {
  const cargoArgs = ['build', '-p', 'talkr-engine', '--locked']
  if (!debug) cargoArgs.push('--release')
  if (crossTarget) cargoArgs.push('--target', crossTarget)
  if (features) cargoArgs.push('--features', features)
  const dir = features === 'vulkan' ? gpuTargetDir : targetDir
  console.log(`[build-engine] cargo ${cargoArgs.join(' ')} (target dir ${dir})`)
  execFileSync('cargo', cargoArgs, { cwd: tauriDir, env: { ...buildEnv(), CARGO_TARGET_DIR: dir }, stdio: 'inherit' })
  return join(dir, ...(crossTarget ? [crossTarget] : []), debug ? 'debug' : 'release', `talkr-engine${exe}`)
}

function install(built, name) {
  const dest = join(binDir, `${name}-${triple}${exe}`)
  copyFileSync(built, dest)
  console.log(`[build-engine] ${dest}`)
}

function vulkanAvailable() {
  // The LunarG SDK (VULKAN_SDK) brings headers and glslc. On Linux whisper-rs-sys links the
  // system loader, so libvulkan-dev is needed as well.
  if (process.env.VULKAN_SDK && existsSync(process.env.VULKAN_SDK)) return true
  if (isWindows) return false
  try {
    execFileSync('glslc', ['--version'], { stdio: 'ignore' })
    execFileSync('pkg-config', ['--exists', 'vulkan'], { stdio: 'ignore' })
    return true
  } catch {
    return false
  }
}

// GPU build first: both builds write target/.../talkr-engine, so each is copied out right away.
let gpuBuilt = false
if (!isMac) {
  if (vulkanAvailable()) {
    install(cargoBuild('vulkan'), 'talkr-engine-gpu')
    gpuBuilt = true
  } else if (process.env.TALKR_REQUIRE_GPU === '1') {
    console.error('[build-engine] The Vulkan SDK is required (VULKAN_SDK, or glslc + libvulkan-dev on Linux).')
    process.exit(1)
  } else {
    console.warn('[build-engine] Vulkan SDK not found: the GPU engine will be a copy of the CPU one.')
  }
}

const cpu = cargoBuild()
install(cpu, 'talkr-engine')

// tauri.windows.conf.json bundles sherpa-onnx's DLLs from target/release, and tauri-build checks
// those paths on every build, debug and `cargo check` included. A debug engine build leaves them
// in target/debug, so mirror them into target/release when they're not there yet.
if (isWindows && debug && !crossTarget) {
  const release = join(targetDir, 'release')
  mkdirSync(release, { recursive: true })
  for (const dll of ['sherpa-onnx-c-api.dll', 'onnxruntime.dll', 'onnxruntime_providers_shared.dll', 'cargs.dll']) {
    const from = join(outDir, dll)
    if (existsSync(from) && !existsSync(join(release, dll))) copyFileSync(from, join(release, dll))
  }
}
if (!isMac && !gpuBuilt) {
  // Tauri needs every externalBin to exist. A CPU copy reports no GPU, so the app simply stays
  // on the CPU (see engine_host.rs).
  install(cpu, 'talkr-engine-gpu')
}
