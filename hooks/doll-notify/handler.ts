const DEFAULT_PORT = 3000;

function resolveDollPort(): number {
	try {
		const home = process.env.HOME ?? process.env.USERPROFILE ?? "";
		const configDir = process.env.XDG_CONFIG_HOME ?? `${home}/.config`;
		const configPath = `${configDir}/doll/config.toml`;
		const content = require("node:fs").readFileSync(configPath, "utf-8");
		const match = (content as string).match(/^port\s*=\s*(\d+)/m);
		if (match) return Number(match[1]);
	} catch {
		// config not found — use default
	}
	return DEFAULT_PORT;
}

const DOLL_PORT = resolveDollPort();

const handler = async (event: { type: string; action: string }) => {
	if (event.type !== "message" || event.action !== "preprocessed") return;

	await fetch(`http://127.0.0.1:${DOLL_PORT}/status`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ status: "responding", emotion: "thinking" }),
	}).catch(() => {});
};

export default handler;
