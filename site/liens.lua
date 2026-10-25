-- Liens des pages du site : guide et FAQ vers leurs pages HTML, autres
-- fichiers du dépôt vers GitHub. Le chemin de la page (variable « source »,
-- relative à docs/) sert à résoudre les liens relatifs.
local depot = "https://github.com/Asfax03L/openheadset-control/blob/main/"

local pages = {
  ["docs/GUIDE.md"] = "guide.html",
  ["docs/FAQ.md"] = "faq.html",
  ["docs/GUIDE.en.md"] = "en/guide.html",
  ["docs/FAQ.en.md"] = "en/faq.html",
}

local racine = ""

-- Résout « ../README.md » depuis « docs/ ».
local function resoudre(cible)
  local chemin = "docs/" .. cible
  local avant
  repeat
    avant = chemin
    chemin = chemin:gsub("[^/]+/%.%./", "", 1)
  until chemin == avant
  return chemin
end

function Meta(meta)
  racine = pandoc.utils.stringify(meta.racine or "")
end

function Link(lien)
  local cible = lien.target
  if cible:match("^%a[%w+.-]*:") or cible:match("^#") then
    return lien
  end
  local fichier, ancre = cible:match("^([^#]*)(#?.*)$")
  -- Seuls les documents Markdown du dépôt sont réécrits ; les pages du
  -- site (« guide.html ») restent telles quelles.
  if not fichier:match("%.md$") then
    return lien
  end
  local chemin = resoudre(fichier)
  if pages[chemin] then
    lien.target = racine .. pages[chemin] .. ancre
  else
    lien.target = depot .. chemin .. ancre
  end
  return lien
end

return {
  { Meta = Meta },
  { Link = Link },
}
