/** Talks to a running app's phone server the way phones do and prints "phone check OK" when it answers as it should. It talks only
 * to the port the app under test was given (KARA_PHONE_PORT), never to the user's own app. Run with NODE_TLS_REJECT_UNAUTHORIZED=0. */
const code = process.env.KARA_PHONE_CODE ?? "";
const origin = `https://127.0.0.1:${process.env.KARA_PHONE_PORT ?? "8543"}`;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

interface Phone { ws: WebSocket; got: { t: string; [k: string]: unknown }[]; closed: Promise<number> }

/** Joins as phone `id` from the phone page's origin and keeps the connection alive with a ping every second, as the phone page does. */
function phone(origin: string, id: string, joinCode = code): Promise<Phone> {
  const ws = new WebSocket(`${origin.replace("https", "wss")}/ws`, { headers: { origin } });
  ws.binaryType = "arraybuffer";
  const got: Phone["got"] = [];
  const ping = setInterval(() => ws.readyState === WebSocket.OPEN && ws.send(JSON.stringify({ t: "ping" })), 1000);
  const closed = new Promise<number>((done) => ws.addEventListener("close", (e) => (clearInterval(ping), done(e.code))));
  ws.addEventListener("message", (e) => typeof e.data === "string" && got.push(JSON.parse(e.data)));
  return new Promise((ok, fail) => {
    ws.addEventListener("open", () => {
      ws.send(JSON.stringify({ t: "join", code: joinCode, id, name: `Guest ${id}` }));
      ok({ ws, got, closed });
    });
    ws.addEventListener("error", () => fail(new Error(`phone ${id} couldn't connect`)));
  });
}

async function until(test: () => boolean, what: string) {
  for (let i = 0; i < 50 && !test(); i++) await wait(100);
  if (!test()) throw new Error(`timed out waiting for ${what}`);
}

const heard = (p: Phone, t: string) => p.got.some((m) => m.t === t);

/** Whether the server at `origin` lets a phone in with our code. */
async function accepts(origin: string): Promise<boolean> {
  try {
    const p = await phone(origin, "probe");
    for (let i = 0; i < 20; i++) {
      if (heard(p, "joined")) {
        p.ws.send(JSON.stringify({ t: "leave" }));
        await p.closed;
        return true;
      }
      if (p.ws.readyState > WebSocket.OPEN) return false;
      await wait(100);
    }
    p.ws.close();
  } catch {
    return false;
  }
  return false;
}

/** Waits until the app under test has its phone server up. */
async function ready() {
  for (let i = 0; i < 30; i++) {
    if (await accepts(origin)) return;
    await wait(1000);
  }
  throw new Error("the app under test never took our code");
}

await ready();
const html = await (await fetch(`${origin}/phone`)).text();
const script = html.match(/["'](?:\.\/|\/)(_app\/[^"']+\.js)["']/)?.[1];
if (!html.includes("<html") || !script || !(await fetch(`${origin}/${script}`)).ok) throw new Error("the phone page or its files didn't load");
const wrong = await phone(origin, "wrong", code === "0000" ? "1111" : "0000");
if ((await wrong.closed) !== 4003) throw new Error("a wrong code was not refused");
const guests = await Promise.all(["1", "2", "3", "4"].map((id) => phone(origin, id)));
for (const g of guests) await until(() => heard(g, "joined") && heard(g, "player"), "joined and the queue");
const fifth = await phone(origin, "5");
if ((await fifth.closed) !== 4002) throw new Error("a fifth phone was not refused");
const first = guests[0];
first.ws.send(JSON.stringify({ t: "leave" }));
await first.closed;
const again = await phone(origin, "6");
await until(() => heard(again, "joined"), "a phone joining after one left");

console.log("phone check OK");
for (const g of [...guests.slice(1), again]) g.ws.close();
