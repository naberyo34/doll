import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion, useAnimate } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import "./App.css";

interface OpenClawStatus {
	status: "idle" | "responding";
	emotion?: string;
}

interface EmotionEntry {
	name: string;
	description: string;
}

interface SkinInfo {
	name: string;
	display_name: string;
	emotions: EmotionEntry[];
}

/** Seconds to wait after the last status update before returning to idle. */
const IDLE_TIMEOUT_SECS = 10;

async function loadSkinImage(emotion: string): Promise<string> {
	const bytes: ArrayBuffer = await invoke("get_skin_image", { emotion });
	const blob = new Blob([bytes], { type: "image/png" });
	return URL.createObjectURL(blob);
}

function App() {
	const [status, setStatus] = useState<OpenClawStatus>({ status: "idle" });
	const [menuOpen, setMenuOpen] = useState(false);
	const [imageUrl, setImageUrl] = useState<string>("");
	const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const imageCache = useRef<Map<string, string>>(new Map());
	const [scope, animate] = useAnimate<HTMLImageElement>();

	const playPoyo = useCallback(() => {
		animate(
			scope.current,
			{ scaleX: [1, 1.06, 0.97, 1.02, 1], scaleY: [1, 0.95, 1.03, 0.99, 1] },
			{ duration: 0.35, ease: "easeOut" },
		);
	}, [animate, scope]);

	const resolveImage = useCallback(
		async (emotion: string) => {
			const cached = imageCache.current.get(emotion);
			if (cached) {
				setImageUrl((prev) => {
					if (prev !== cached) playPoyo();
					return cached;
				});
				return;
			}
			try {
				const url = await loadSkinImage(emotion);
				imageCache.current.set(emotion, url);
				setImageUrl((prev) => {
					if (prev !== url) playPoyo();
					return url;
				});
			} catch (e) {
				console.error("Failed to load skin image:", e);
				const fallback = imageCache.current.get("idle");
				if (fallback) setImageUrl(fallback);
			}
		},
		[playPoyo],
	);

	useEffect(() => {
		let cancelled = false;

		async function preload() {
			try {
				const idleUrl = await loadSkinImage("idle");
				if (cancelled) return;
				imageCache.current.set("idle", idleUrl);
				setImageUrl(idleUrl);

				const info: SkinInfo = await invoke("get_skin_info");
				if (cancelled) return;
				await Promise.all(
					info.emotions.map(async (entry) => {
						const url = await loadSkinImage(entry.name);
						if (!cancelled) imageCache.current.set(entry.name, url);
					}),
				);
			} catch (e) {
				console.error("Failed to preload skin:", e);
			}
		}

		preload();
		return () => {
			cancelled = true;
		};
	}, []);

	useEffect(() => {
		const unlisten = listen<OpenClawStatus>("openclaw-status", (event) => {
			setStatus(event.payload);

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

	useEffect(() => {
		if (status.status === "responding" && status.emotion) {
			resolveImage(status.emotion);
		} else {
			resolveImage("idle");
		}
	}, [status, resolveImage]);

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

	const handleMouseDown = useCallback(() => {
		getCurrentWindow().startDragging();
	}, []);

	return (
		<div
			role="application"
			className="mascot-container"
			onMouseDown={handleMouseDown}
		>
			{imageUrl && (
				<motion.div
					className="mascot-breathing"
					animate={{
						scaleX: [1, 1.005, 1, 0.995, 1],
						scaleY: [1, 0.995, 1, 1.005, 1],
					}}
					transition={{
						duration: 3.5,
						ease: "easeInOut",
						repeat: Number.POSITIVE_INFINITY,
					}}
				>
					<motion.img
						ref={scope}
						className="mascot-image"
						src={imageUrl}
						alt="mascot"
						onLoad={handleImageLoad}
					/>
				</motion.div>
			)}

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
				<nav className="menu-panel" onMouseDown={(e) => e.stopPropagation()}>
					<button
						type="button"
						onClick={() => invoke("open_config_file").catch(console.error)}
					>
						設定ファイルを開く
					</button>
				</nav>
			)}
		</div>
	);
}

export default App;
