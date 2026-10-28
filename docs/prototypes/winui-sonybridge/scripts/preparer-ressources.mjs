// Téléchargement explicite des images de la maquette ; aucun code distant exécuté.
import { readFile, mkdir, writeFile } from 'node:fs/promises';
const base = new URL('../', import.meta.url);
const ressources = JSON.parse(await readFile(new URL('ressources.json', base), 'utf8'));
const sortie = new URL('public/assets/', base);
await mkdir(sortie, { recursive: true });
for (const ressource of ressources) {
  if (!/^[a-z0-9_-]+\.(svg|png|jpg)$/.test(ressource.file)) throw new Error('Nom de ressource refusé');
  const adresse = new URL(ressource.url);
  if (adresse.protocol !== 'https:' || !['raw.githubusercontent.com', 'cdn.shopify.com'].includes(adresse.hostname)) throw new Error('Origine refusée');
  const reponse = await fetch(adresse, { signal: AbortSignal.timeout(30000) });
  if (!reponse.ok) throw new Error(`Ressource indisponible : ${ressource.file} (${reponse.status})`);
  await writeFile(new URL(ressource.file, sortie), Buffer.from(await reponse.arrayBuffer()));
  console.log(`Ressource locale préparée : ${ressource.file}`);
}
