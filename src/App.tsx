import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import neutralImage from "./assets/tama/neutral.png";
import "./App.css";

/** Status pushed from the Rust backend via Tauri events. */
interface OpenClawStatus {
	status: "idle" | "responding";
	emotion?: "happy" | "sad" | "angry" | "surprised" | "neutral";
}

const EMOTION_LABELS: Record<string, string> = {
	happy: "😊 嬉しい",
	sad: "😢 悲しい",
	angry: "😠 怒り",
	surprised: "😲 驚き",
	neutral: "",
};

function statusToLabel(s: OpenClawStatus): string {
	if (s.status === "responding" && s.emotion) {
		return EMOTION_LABELS[s.emotion] ?? s.emotion;
	}
	return "";
}

/** Seconds to wait after the last status update before returning to idle. */
const IDLE_TIMEOUT_SECS = 10;

function App() {
	const [status, setStatus] = useState<OpenClawStatus>({ status: "idle" });
	const [menuOpen, setMenuOpen] = useState(false);
	const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

	// Listen for status events from Rust backend
	useEffect(() => {
		const unlisten = listen<OpenClawStatus>("openclaw-status", (event) => {
			setStatus(event.payload);

			// Reset the idle timer on every incoming status update.
			if (idleTimer.current) clearTimeout(idleTimer.current);
			if (event.payload.status !== "idle") {
				idleTimer.current = setTimeout(() => {
					setStatus({ status: "idle" });
				}, IDLE_TIMEOUT_SECS * 1000);
			}
		});
		return () => {
			unlisten.then((fn) => fn());
			if (idleTimer.current) clearTimeout(idleTimer.current);
		};
	}, []);

	// When the sprite image loads, resize the window to fit it
	const handleImageLoad = useCallback(
		(e: React.SyntheticEvent<HTMLImageElement>) => {
			const img = e.currentTarget;
			const w = img.clientWidth;
			const h = img.clientHeight;
			if (w > 0 && h > 0) {
				getCurrentWindow().setSize(new LogicalSize(w, h));
			}
		},
		[],
	);

	// Drag the window by mouse-down anywhere on the container
	const handleMouseDown = useCallback(() => {
		getCurrentWindow().startDragging();
	}, []);

	const label = statusToLabel(status);

	return (
		<div
			role="application"
			className="mascot-container"
			onMouseDown={handleMouseDown}
		>
			<img
				className="mascot-image"
				src={neutralImage}
				alt="mascot"
				onLoad={handleImageLoad}
			/>
			<span className={`status-label ${label ? "visible" : ""}`}>{label}</span>

			{/* Menu toggle */}
			<button
				type="button"
				className="menu-toggle"
				onMouseDown={(e) => e.stopPropagation()}
				onClick={() => setMenuOpen((v) => !v)}
				title="メニュー"
			>
				⚙
			</button>

			{menuOpen && (
				<div
					className="menu-panel"
					onMouseDown={(e) => e.stopPropagation()}
				>
					<button
						type="button"
						onClick={() => invoke("open_config_file").catch(console.error)}
					>
						設定ファイルを開く
					</button>
				</div>
			)}
		</div>
	);
}

export default App;
