import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { motion, useAnimate } from "motion/react";
import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { useOpenClawStatus } from "./hooks/useOpenClawStatus";
import { useSkinLoader } from "./hooks/useSkinLoader";

function App() {
	const [menuOpen, setMenuOpen] = useState(false);
	const [scope, animate] = useAnimate<HTMLImageElement>();

	const playPoyo = useCallback(() => {
		animate(
			scope.current,
			{ scaleX: [1, 1.06, 0.97, 1.02, 1], scaleY: [1, 0.95, 1.03, 0.99, 1] },
			{ duration: 0.35, ease: "easeOut" },
		);
	}, [animate, scope]);

	const status = useOpenClawStatus();
	const { imageUrl, resolveImage } = useSkinLoader(playPoyo);

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
					<button
						type="button"
						onClick={() => invoke("quit_app").catch(console.error)}
					>
						終了する
					</button>
				</nav>
			)}
		</div>
	);
}

export default App;
