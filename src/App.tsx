import { invoke } from "@tauri-apps/api/core";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AnimatePresence, motion, useAnimate } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import "./App.css";
import { useOpenClawStatus } from "./hooks/useOpenClawStatus";
import { useSkinLoader } from "./hooks/useSkinLoader";

const DRAG_THRESHOLD = 4;
const MAX_LOG_ITEMS = 4;

interface ChatEntry {
	id: number;
	role: "user" | "assistant";
	text: string;
}

let nextEntryId = 0;

function App() {
	const [scope, animate] = useAnimate<HTMLImageElement>();
	const [chatOpen, setChatOpen] = useState(false);
	const [sending, setSending] = useState(false);
	const [chatLog, setChatLog] = useState<ChatEntry[]>([]);
	const chatInputRef = useRef<HTMLInputElement>(null);
	const pointerOrigin = useRef<{ x: number; y: number } | null>(null);
	const dragged = useRef(false);
	const lastReceivedText = useRef("");

	const pushEntry = useCallback((role: "user" | "assistant", text: string) => {
		setChatLog((prev) =>
			[...prev, { id: nextEntryId++, role, text }].slice(-MAX_LOG_ITEMS),
		);
	}, []);

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

	useEffect(() => {
		const text = status.text;
		if (text && text !== lastReceivedText.current) {
			lastReceivedText.current = text;
			pushEntry("assistant", text);
		}
	}, [status.text, pushEntry]);

	useEffect(() => {
		if (chatOpen && chatInputRef.current) {
			chatInputRef.current.focus();
		}
	}, [chatOpen]);

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

	const handlePointerDown = useCallback((e: React.PointerEvent) => {
		if ((e.target as HTMLElement).closest(".overlay-strip")) return;
		pointerOrigin.current = { x: e.clientX, y: e.clientY };
		dragged.current = false;
	}, []);

	const handlePointerMove = useCallback((e: React.PointerEvent) => {
		if (!pointerOrigin.current || dragged.current) return;
		const dx = e.clientX - pointerOrigin.current.x;
		const dy = e.clientY - pointerOrigin.current.y;
		if (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD) {
			dragged.current = true;
			getCurrentWindow().startDragging();
		}
	}, []);

	const handlePointerUp = useCallback(() => {
		if (!dragged.current && pointerOrigin.current) {
			setChatOpen((prev) => !prev);
		}
		pointerOrigin.current = null;
	}, []);

	const handleContextMenu = useCallback((e: React.MouseEvent) => {
		e.preventDefault();
		invoke("show_context_menu").catch(console.error);
	}, []);

	const handleSend = useCallback(
		async (e: React.FormEvent) => {
			e.preventDefault();
			const input = chatInputRef.current;
			if (!input || !input.value.trim() || sending) return;
			const text = input.value.trim();
			pushEntry("user", text);
			input.value = "";
			setSending(true);
			try {
				await invoke("send_message", { text });
				setChatOpen(false);
			} catch (err) {
				console.error("send_message failed:", err);
			} finally {
				setSending(false);
			}
		},
		[sending, pushEntry],
	);

	const hasOverlay = chatLog.length > 0 || chatOpen;

	return (
		<div
			role="application"
			className="mascot-container"
			onPointerDown={handlePointerDown}
			onPointerMove={handlePointerMove}
			onPointerUp={handlePointerUp}
			onContextMenu={handleContextMenu}
		>
			<AnimatePresence>
				{hasOverlay && (
					<motion.div
						className="overlay-strip"
						key="overlay"
						initial={{ opacity: 0, y: 8 }}
						animate={{ opacity: 1, y: 0 }}
						exit={{ opacity: 0, y: 8 }}
						transition={{ duration: 0.2, ease: "easeOut" }}
					>
						<div className="chat-log" aria-live="polite">
							<AnimatePresence initial={false}>
								{chatLog.map((entry) => (
									<motion.div
										className={`chat-entry chat-entry--${entry.role}`}
										key={entry.id}
										initial={{ opacity: 0, y: 10 }}
										animate={{ opacity: 1, y: 0 }}
										exit={{ opacity: 0, height: 0, marginBottom: 0 }}
										transition={{ duration: 0.2, ease: "easeOut" }}
									>
										<p>{entry.text}</p>
									</motion.div>
								))}
							</AnimatePresence>
						</div>
						<AnimatePresence>
							{chatOpen && (
								<motion.form
									className="chat-bar"
									key="chat"
									onSubmit={handleSend}
									initial={{ opacity: 0, scale: 0.95 }}
									animate={{ opacity: 1, scale: 1 }}
									exit={{ opacity: 0, scale: 0.95 }}
									transition={{ duration: 0.2, ease: "easeOut" }}
								>
									<input
										ref={chatInputRef}
										className="chat-input"
										type="text"
										placeholder="メッセージを送信..."
										disabled={sending}
									/>
								</motion.form>
							)}
						</AnimatePresence>
					</motion.div>
				)}
			</AnimatePresence>
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
		</div>
	);
}

export default App;
