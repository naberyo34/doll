const DOLL_HOST = process.env.DOLL_HOST ?? "127.0.0.1";
const DOLL_PORT = Number(process.env.DOLL_PORT) || 3000;

const handler = async (event: { type: string; action: string }) => {
	if (event.type !== "message" || event.action !== "preprocessed") return;

	await fetch(`http://${DOLL_HOST}:${DOLL_PORT}/status`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ status: "responding", emotion: "thinking" }),
	}).catch(() => {});
};

export default handler;
