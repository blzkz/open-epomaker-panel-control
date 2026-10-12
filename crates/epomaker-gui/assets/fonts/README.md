# Fonts

Fira Sans Regular, Fira Sans SemiBold and Fira Sans Condensed Bold (version 4.301,
<https://github.com/mozilla/Fira>), licensed under the SIL Open Font License 1.1
(`OFL.txt`). They are embedded in `epomaker-panel`, reduced to Latin scripts and common
punctuation with:

```sh
pyftsubset FiraSans-Regular.ttf --output-file=FiraSans-Regular.ttf \
  --unicodes="U+0020-007E,U+00A0-024F,U+0300-036F,U+2010-2027,U+2030-203A,U+2044,U+20AC,U+2116,U+2122,U+2190-2195,U+21B5,U+2212,U+2215,U+2318,U+2325,U+2387,U+238B,U+2C60-2C7F,U+1E00-1EFF" \
  --layout-features='kern,liga,calt,ccmp,locl,mark,mkmk,tnum'
```

Icons come from the Phosphor icon font bundled by the `egui-phosphor` crate (MIT).
