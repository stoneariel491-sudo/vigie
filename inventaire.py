import re, glob, sys

fr = re.compile(r"\b(le|la|les|un|une|des|du|de|en|et|ou|pas|sur|dans|pour|avec|sans|est|aucun|aucune|cours|geste|gestes|image|images|projet|projets|fiche|ouvrir|page|lien|lanc|termin|interrompu|Direct|Aujourd|hier|agent|agents|chantier|chantiers|minute|heure|jour|seconde|re[cç]ue|ouverte|produite|regard)\b", re.I)
lit_re = re.compile(r'"((?:[^"\\]|\\.)*)"')
total = 0
detail = []
for f in sorted(glob.glob('src/*.rs')) + sorted(glob.glob('src/import/*.rs')):
    lignes = open(f, encoding='utf-8').read().split('\n')
    en_test = False
    n = 0
    for i, l in enumerate(lignes):
        s = l.strip()
        if s.startswith('#[cfg(test)]'):
            en_test = True
        if en_test or s.startswith('//'):
            continue
        for m in lit_re.finditer(l):
            lit = m.group(1)
            if len(lit) > 3 and fr.search(lit) and not lit.startswith(('--', 'VIGIE', 'C:')):
                n += 1
                detail.append((f, i + 1, lit[:90]))
    if n:
        print(f, n)
    total += n
print('total', total)
if len(sys.argv) > 1:
    for d in detail:
        print('%s:%d: %s' % d)
