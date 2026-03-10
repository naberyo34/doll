import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

interface EmotionEntry {
	name: string;
	description: string;
}

interface SkinInfo {
	name: string;
	display_name: string;
	emotions: EmotionEntry[];
}

async function loadSkinImage(emotion: string): Promise<string> {
	const bytes: ArrayBuffer = await invoke("get_skin_image", { emotion });
	const blob = new Blob([bytes], { type: "image/png" });
	return URL.createObjectURL(blob);
}

export function useSkinLoader(onEmotionChange: () => void) {
	const [imageUrl, setImageUrl] = useState("");
	const imageCache = useRef<Map<string, string>>(new Map());

	const revokeAll = useCallback(() => {
		for (const url of imageCache.current.values()) {
			URL.revokeObjectURL(url);
		}
		imageCache.current.clear();
	}, []);

	const resolveImage = useCallback(
		async (emotion: string) => {
			const cached = imageCache.current.get(emotion);
			if (cached) {
				setImageUrl((prev) => {
					if (prev !== cached) onEmotionChange();
					return cached;
				});
				return;
			}
			try {
				const url = await loadSkinImage(emotion);
				imageCache.current.set(emotion, url);
				setImageUrl((prev) => {
					if (prev !== url) onEmotionChange();
					return url;
				});
			} catch (e) {
				console.error("Failed to load skin image:", e);
				const fallback = imageCache.current.get("idle");
				if (fallback) setImageUrl(fallback);
			}
		},
		[onEmotionChange],
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
			revokeAll();
		};
	}, [revokeAll]);

	return { imageUrl, resolveImage };
}
