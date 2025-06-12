build-web:
  just web/build
  mkdir -p docs
  mv web/docs/public/* docs
  cp docs/index.html docs/404.html
  # Add and commit with git 
  git add docs
  git commit -m "Update docs" || true
