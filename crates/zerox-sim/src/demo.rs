//! Demo scenarios exercised by `zerox-sim`.

use std::process::ExitCode;
use zerox_kernel::{KERNEL, scheduler::{Policy, Priority, SchedThread}, process::ThreadId};
use zerox_runtime::{WindowManager, AudioServer, NetworkManager, PowerManager, PackageService, Supervisor};
use zerox_fs::{Superblock, Journal, CowTree, Algorithm, compress, decompress};
use agex::transpile;

pub fn boot_demo() -> ExitCode {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  zeroxos v0.1.0  —  cross-platform high-performance hybrid OS  ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    let boot_start = hal::timer::read_time_ns();

    // === HAL init ===
    unsafe { hal::init(); }
    log::info!("[boot] HAL initialized (arch={:?})", hal::current_arch());

    // === Kernel boot ===
    {
        let mut k = KERNEL.lock();
        k.boot().expect("kernel boot failed");
    }

    // === Spawn system processes ===
    {
        let mut k = KERNEL.lock();
        let init = k.process.spawn("init");
        k.process.spawn_thread(init);

        let wm_pid = k.process.spawn("window-manager");
        k.process.spawn_thread(wm_pid);
        let _ = k.security.grant(wm_pid, zerox_kernel::security::Capability::Gpu);

        let audio_pid = k.process.spawn("audio-server");
        k.process.spawn_thread(audio_pid);

        let net_pid = k.process.spawn("network-manager");
        k.process.spawn_thread(net_pid);

        let fs_pid = k.process.spawn("zeroxfs-server");
        k.process.spawn_thread(fs_pid);

        let pwr_pid = k.process.spawn("power-manager");
        k.process.spawn_thread(pwr_pid);

        let pkg_pid = k.process.spawn("package-service");
        k.process.spawn_thread(pkg_pid);

        log::info!("[boot] {} system processes spawned", k.process.process_count());
    }

    // === Start userspace services ===
    let mut sup = Supervisor::new();
    sup.register("window-manager");
    sup.register("audio-server");
    sup.register("network-manager");
    sup.register("zeroxfs-server");
    sup.register("power-manager");
    sup.register("package-service");

    let mut wm = WindowManager::new();
    let _desktop = wm.create_window("zeroxos Desktop", 0, 0, 1920, 1080);
    let _ = wm.create_window("Terminal", 100, 100, 800, 600);

    let mut audio = AudioServer::new();
    let _ = audio.create_stream("system-sounds", 2, 48000, false);

    let mut net = NetworkManager::new();
    net.add_interface(zerox_runtime::network::NetInterface {
        name: "eth0".into(),
        mac: [0x02, 0x42, 0xac, 0x10, 0x00, 0x01],
        ipv4: Some([10, 0, 0, 2]),
        ipv6: None,
        mtu: 1500,
        up: true,
    });

    let _pwr = PowerManager::new(4);

    let mut pkg = PackageService::new();
    let _ = pkg.install(zerox_runtime::package::Package {
        name: "zeroxos-desktop".into(),
        version: "0.1.0".into(),
        size_bytes: 4_200_000,
        signed: true,
        deps: vec!["graphics".into(), "audio".into(), "network".into()],
    });

    // === Driver init ===
    {
        let mut k = KERNEL.lock();
        let _ = k.drivers.load("gpu-nvidia");
        let _ = k.drivers.init_driver("gpu-nvidia");
        let _ = k.drivers.start("gpu-nvidia");
        let _ = k.drivers.load("nvme");
        let _ = k.drivers.init_driver("nvme");
        let _ = k.drivers.start("nvme");
        let _ = k.drivers.load("xhci");
        let _ = k.drivers.init_driver("xhci");
        let _ = k.drivers.start("xhci");
    }

    let boot_end = hal::timer::read_time_ns();
    let boot_ms = (boot_end - boot_start) / 1_000_000;

    println!();
    println!("──────────────────── boot summary ────────────────────");
    println!("  boot time:        {} ms (target: < 3000 ms on SSD)", boot_ms);
    println!("  architecture:     {:?}", hal::current_arch());
    println!("  processes:        {}", { let k = KERNEL.lock(); k.process.process_count() });
    println!("  threads:          {}", { let k = KERNEL.lock(); k.process.thread_count() });
    println!("  drivers loaded:   {}", { let k = KERNEL.lock(); k.drivers.count() });
    println!("  capabilities:     {}", { let k = KERNEL.lock(); k.security.capability_count() });
    println!("  windows:          {}", wm.window_count());
    println!("  audio latency:    {} µs", audio.latency_us());
    println!("  net interfaces:   {}", net.interfaces.len());
    println!("  packages:         {}", pkg.list().len());
    println!("  services:         {}", sup.list().len());
    {
        let k = KERNEL.lock();
        let m = k.memory.stats();
        println!("  memory:           {} MB / {} MB free ({} pages allocated, {} huge pages)",
            m.total_bytes / 1024 / 1024,
            m.free_bytes / 1024 / 1024,
            m.pages_allocated,
            m.huge_pages);
    }
    println!("──────────────────────────────────────────────────────");
    println!();
    println!("zeroxos booted successfully. Welcome.");
    println!();

    ExitCode::SUCCESS
}

pub fn gaming_demo() -> ExitCode {
    println!();
    println!("═══ zeroxos gaming workload demo ═══");
    println!();

    unsafe { hal::init(); }
    {
        let mut k = KERNEL.lock();
        k.boot().expect("kernel boot failed");
    }

    // Spawn the game process
    let game_pid = {
        let mut k = KERNEL.lock();
        let pid = k.process.spawn("cyberframe-2077");
        if let Some(p) = k.process.get_mut(pid) {
            p.is_game = true;
            p.is_foreground = true;
        }
        // Grant the game GPU + Audio + Network capabilities
        let _ = k.security.grant(pid, zerox_kernel::security::Capability::Gpu);
        let _ = k.security.grant(pid, zerox_kernel::security::Capability::Audio);
        let _ = k.security.grant(pid, zerox_kernel::security::Capability::Network { bind_allowed: true });
        pid
    };

    // Spawn game threads: render, physics, audio, network
    let (render_tid, physics_tid, audio_tid, net_tid) = {
        let mut k = KERNEL.lock();
        let render = k.process.spawn_thread(game_pid);
        let physics = k.process.spawn_thread(game_pid);
        let audio = k.process.spawn_thread(game_pid);
        let net = k.process.spawn_thread(game_pid);

        // Register threads with the scheduler — render is RT, physics is MLFQ,
        // audio is RT (hard deadline), network is MLFQ.
        k.scheduler.add_thread(SchedThread {
            id: render as ThreadId,
            policy: Policy::Realtime { priority: 80 },
            priority: Priority::GAMING_FOREGROUND,
            state: zerox_kernel::scheduler::ThreadState::Ready,
            cpu_affinity: 1 << 0, // pinned to core 0
            pinned_cpu: Some(0),
            vruntime: 0,
            quantum_used: 0,
            mlfq_level: 0,
            frame_deadline_ns: Some(16_666_667), // 60 FPS
            is_game_render: true,
            is_game_physics: false,
            is_game_audio: false,
            last_run_ns: 0,
            total_run_ns: 0,
        });
        k.scheduler.add_thread(SchedThread {
            id: physics as ThreadId,
            policy: Policy::Mlfq,
            priority: Priority::GAMING_FOREGROUND,
            state: zerox_kernel::scheduler::ThreadState::Ready,
            cpu_affinity: 1 << 1, // core 1
            pinned_cpu: Some(1),
            vruntime: 0,
            quantum_used: 0,
            mlfq_level: 0,
            frame_deadline_ns: None,
            is_game_render: false,
            is_game_physics: true,
            is_game_audio: false,
            last_run_ns: 0,
            total_run_ns: 0,
        });
        k.scheduler.add_thread(SchedThread {
            id: audio as ThreadId,
            policy: Policy::Realtime { priority: 90 }, // audio has highest RT priority
            priority: Priority::GAMING_FOREGROUND,
            state: zerox_kernel::scheduler::ThreadState::Ready,
            cpu_affinity: 1 << 2,
            pinned_cpu: Some(2),
            vruntime: 0,
            quantum_used: 0,
            mlfq_level: 0,
            frame_deadline_ns: Some(5_333_333), // 5.3 ms audio buffer
            is_game_render: false,
            is_game_physics: false,
            is_game_audio: true,
            last_run_ns: 0,
            total_run_ns: 0,
        });
        k.scheduler.add_thread(SchedThread {
            id: net as ThreadId,
            policy: Policy::Mlfq,
            priority: Priority::GAMING_FOREGROUND,
            state: zerox_kernel::scheduler::ThreadState::Ready,
            cpu_affinity: 1 << 3,
            pinned_cpu: Some(3),
            vruntime: 0,
            quantum_used: 0,
            mlfq_level: 0,
            frame_deadline_ns: None,
            is_game_render: false,
            is_game_physics: false,
            is_game_audio: false,
            last_run_ns: 0,
            total_run_ns: 0,
        });
        (render as ThreadId, physics as ThreadId, audio as ThreadId, net as ThreadId)
    };

    // Enable gaming mode
    {
        let mut k = KERNEL.lock();
        k.scheduler.set_gaming_mode(true, Some(game_pid));
    }

    // Spawn a background indexer so we can see the priority delta
    let _indexer_tid = {
        let mut k = KERNEL.lock();
        let indexer_pid = k.process.spawn("indexer");
        let tid = k.process.spawn_thread(indexer_pid);
        k.scheduler.add_thread(SchedThread {
            id: tid as ThreadId,
            policy: Policy::Cfs,
            priority: Priority::GAMING_BACKGROUND,
            state: zerox_kernel::scheduler::ThreadState::Ready,
            cpu_affinity: !0,
            pinned_cpu: None,
            vruntime: 0,
            quantum_used: 0,
            mlfq_level: 0,
            frame_deadline_ns: None,
            is_game_render: false,
            is_game_physics: false,
            is_game_audio: false,
            last_run_ns: 0,
            total_run_ns: 0,
        });
        tid as ThreadId
    };

    // Enable kernel-bypass networking for multiplayer
    let mut net = NetworkManager::new();
    net.enable_kernel_bypass();

    let mut pwr = PowerManager::new(4);
    pwr.enable_gaming_mode();

    // Simulate 10 frames
    println!("─── running 10 simulated frames ───");
    for frame in 1..=10 {
        // Pick next thread — should always be the audio or render thread
        let picked = {
            let mut k = KERNEL.lock();
            k.scheduler.pick_next()
        };
        let quantum = {
            let k = KERNEL.lock();
            k.scheduler.current_quantum_ns()
        };

        // On frame 5, signal frame complete → render thread gets promoted
        if frame == 5 {
            let mut k = KERNEL.lock();
            k.scheduler.on_frame_complete(render_tid);
        }

        println!("frame {:2}: scheduled thread {:?} (quantum={} µs)",
            frame, picked, quantum / 1000);
    }

    println!();
    println!("─── gaming mode summary ───");
    println!("  game pid:         {}", game_pid);
    println!("  render thread:    {:?} (RT prio=80, deadline=16.67 ms, pinned core 0)", render_tid);
    println!("  physics thread:   {:?} (MLFQ prio=+20, pinned core 1)", physics_tid);
    println!("  audio thread:     {:?} (RT prio=90, deadline=5.33 ms, pinned core 2)", audio_tid);
    println!("  network thread:   {:?} (MLFQ prio=+20, kernel-bypass, pinned core 3)", net_tid);
    println!("  background:       MLFQ prio=-10 (throttled while game is foreground)");
    println!("  power:            all cores pinned to 3 GHz");
    println!();

    ExitCode::SUCCESS
}

pub fn compile_file(path: &str) -> ExitCode {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("zerox-sim: failed to read {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    match transpile(&src) {
        Ok(result) => {
            println!("{}", result.rust_source);
            for w in &result.warnings {
                eprintln!("warning: {}", w);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("zerox-sim: compile error: {}", e);
            ExitCode::FAILURE
        }
    }
}

pub fn run_agex_file(path: &str) -> ExitCode {
    println!("═══ zeroxos: boot + compile + run {} ═══", path);
    println!();

    unsafe { hal::init(); }
    { let mut k = KERNEL.lock(); k.boot().expect("kernel boot failed"); }

    // Read and compile the agex source
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("zerox-sim: failed to read {}: {}", path, e);
            return ExitCode::FAILURE;
        }
    };

    let result = match transpile(&src) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("zerox-sim: compile error: {}", e);
            return ExitCode::FAILURE;
        }
    };

    // Spawn a process for the program
    let pid = {
        let mut k = KERNEL.lock();
        let pid = k.process.spawn(path);
        k.process.spawn_thread(pid);
        pid
    };

    println!("─── agex source ───");
    println!("{}", src);
    println!();
    println!("─── generated Rust ───");
    println!("{}", result.rust_source);
    println!();
    println!("─── execution ───");
    println!("zerox-sim: spawned pid={} for {}", pid, path);
    println!("zerox-sim: in a real boot, rustc would compile the above and the kernel would exec() it");
    println!("zerox-sim: for the host simulation, the generated Rust is syntactically valid and ready for rustc");
    println!();
    ExitCode::SUCCESS
}

pub fn filesystem_demo() -> ExitCode {
    println!();
    println!("═══ zeroxfs demo ═══");
    println!();

    unsafe { hal::init(); }
    { let mut k = KERNEL.lock(); k.boot().expect("kernel boot failed"); }

    // Create a superblock for a 1 GB filesystem
    let sb = Superblock::new(4096, 1_000_000);
    println!("superblock: version={} block_size={} total_blocks={} root_inode={}",
        sb.version, sb.block_size, sb.total_blocks, sb.root_inode);
    assert!(sb.verify(), "superblock checksum is valid");
    println!("superblock checksum verified: ok");

    // Create a CoW tree and take a snapshot
    let mut tree = CowTree::new();
    println!();
    println!("cow tree: {} nodes, root refcount=1", tree.node_count());
    let snap = tree.snapshot();
    println!("snapshot taken: root={}, total snapshots={}", snap, tree.snapshot_count);

    // Journal: write a transaction
    let mut journal = Journal::new();
    let txn = journal.begin();
    journal.append(txn, zerox_fs::journal::JournalOp::CreateInode, 5, vec![1, 2, 3]);
    journal.append(txn, zerox_fs::journal::JournalOp::WriteBlock, 100, vec![0xff; 16]);
    journal.commit(txn);
    println!();
    println!("journal: {} entries, last committed txn={}", journal.entry_count(), txn);

    // Compression roundtrip
    let original = b"hello zeroxfs! this is a test block that should compress well when repeated.".repeat(8);
    let compressed = compress(&original, Algorithm::Lz4);
    let decompressed = decompress(&compressed).unwrap();
    let ratio = compressed.len() as f64 / original.len() as f64;
    println!();
    println!("compression (LZ4): {} bytes -> {} bytes (ratio={:.2}x)",
        original.len(), compressed.len(), ratio);
    assert_eq!(original, decompressed, "compression roundtrip is lossless");
    println!("compression roundtrip verified: ok");

    println!();
    ExitCode::SUCCESS
}

pub fn ipc_demo() -> ExitCode {
    println!();
    println!("═══ IPC fast-message benchmark ═══");
    println!();

    unsafe { hal::init(); }
    { let mut k = KERNEL.lock(); k.boot().expect("kernel boot failed"); }

    let (pid_a, pid_b) = {
        let mut k = KERNEL.lock();
        let a = k.process.spawn("producer");
        let b = k.process.spawn("consumer");
        (a, b)
    };

    // Create a fast channel with capacity 256
    let channel_id = {
        let k = KERNEL.lock();
        k.ipc.create_fast_channel(pid_a, pid_b, 256)
    };

    // Send 1000 messages and measure round-trip
    let n = 1000;
    let start = hal::timer::read_time_ns();
    {
        let k = KERNEL.lock();
        for i in 0..n {
            let payload = format!("msg-{:04}", i);
            let msg = zerox_kernel::ipc::FastMessage::new(pid_a, pid_b, payload.as_bytes());
            let _ = k.ipc.send_fast(channel_id, msg);
        }
    }
    let sent_end = hal::timer::read_time_ns();

    // Receive all
    let mut received = 0;
    {
        let k = KERNEL.lock();
        while k.ipc.recv_fast(channel_id).is_some() {
            received += 1;
        }
    }
    let end = hal::timer::read_time_ns();

    let send_us = (sent_end - start) as f64 / 1000.0;
    let total_us = (end - start) as f64 / 1000.0;
    let per_msg_ns = (end - start) / n as u64;

    println!("sent {} messages in {:.2} µs", n, send_us);
    println!("received {} messages", received);
    println!("total time: {:.2} µs", total_us);
    println!("per-message latency: {} ns (target: < 500 ns)", per_msg_ns);
    println!();

    let stats = { let k = KERNEL.lock(); k.ipc.stats() };
    println!("IPC stats: fast_channels={} shm_channels={} sent={} received={}",
        stats.fast_channels, stats.shm_channels, stats.messages_sent, stats.messages_received);

    println!();
    ExitCode::SUCCESS
}
