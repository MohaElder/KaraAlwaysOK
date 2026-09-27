/** Fails when a locale file's keys differ from English's or one of its texts is empty. */
import { readdirSync } from "node:fs";

const dir = new URL("../src/lib/i18n/", import.meta.url);
const load = async (file: string): Promise<Record<string, string>> => (await import(new URL(file, dir).href)).default;

const english = Object.keys(await load("en.ts"));
let problems = 0;
for (const file of readdirSync(dir).filter((f) => /^[a-z]{2}(-[A-Z][a-z]+)?\.ts$/.test(f))) {
  const dict = await load(file);
  const found = [
    ...english.filter((k) => !(k in dict)).map((k) => `missing "${k}"`),
    ...Object.keys(dict).filter((k) => !english.includes(k)).map((k) => `extra "${k}"`),
    ...Object.keys(dict).filter((k) => !dict[k].trim()).map((k) => `empty "${k}"`),
  ];
  for (const p of found) console.error(`${file}: ${p}`);
  problems += found.length;
}
if (problems) process.exit(1);
console.log("Every locale has exactly the English keys, none empty.");
