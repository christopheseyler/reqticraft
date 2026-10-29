#!/usr/bin/env sh
# Refuse les fichiers qui ne doivent jamais entrer dans le dépôt (voir CONTRIBUTING.md) :
# résultats Reqtify, et fichiers marqués comme générés par Reqtify.
set -eu

status=0

binaries=$(git ls-files '*.rqtfimage' '*.chm' '*.pdb')
if [ -n "$binaries" ]; then
  echo "Fichiers interdits :"
  echo "$binaries"
  status=1
fi

# Les fichiers produits par Reqtify portent ce commentaire. Seul docs/ peut le
# contenir : la documentation le cite, et docs/interop-evidence/ garde les fichiers
# enregistrés par Reqtify à partir de nos propres documents de test.
generated=$(git grep -l 'Generator: reqtify' -- ':!docs/' ':!tools/check-clean-room.sh' || true)
if [ -n "$generated" ]; then
  echo "Fichiers générés par Reqtify hors de docs/ :"
  echo "$generated"
  status=1
fi

exit $status
