import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

interface OpenClawStatus {
	status: "idle" | "responding";
	emotion?: string;
	text?: string;
}

const IDLE_TIMEOUT_SECS = 30;

export function useOpenClawStatus() {
	const [status, setStatus] = useState<OpenClawStatus>({ status: "idle" });
	const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

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

	return status;
}
