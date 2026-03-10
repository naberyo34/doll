const DOLL_URL = "http://127.0.0.1:3000/status";

const handler = async (event: {
	type: string;
	action: string;
	context: { content?: string; success?: boolean };
}) => {
	if (event.type !== "message") return;

	if (event.action === "preprocessed") {
		await fetch(DOLL_URL, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ status: "responding", emotion: "thinking" }),
		}).catch(() => {});
		return;
	}

	if (event.action === "sent") {
		if (!event.context.success) return;
		const text = event.context.content?.slice(0, 200) ?? "";
		await fetch(DOLL_URL, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ status: "responding", emotion: "happy", text }),
		}).catch(() => {});
	}
};

export default handler;
