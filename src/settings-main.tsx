import React from 'react'
import ReactDOM from 'react-dom/client'
import SettingsApp from './SettingsApp'
import './styles/globals.css'
import 'flex-design/theme-forge/designer.css'
import { THEMES, DEFAULT_THEME_KEY } from 'flex-design/themes/presets.js'
import { registerTheme, applyTheme, loadThemeChoice, initCustomThemes } from 'flex-design/runtime/theme.js'

for (const t of Object.values(THEMES)) registerTheme(t)
initCustomThemes()
applyTheme(loadThemeChoice('flexfind', DEFAULT_THEME_KEY), document.documentElement)

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SettingsApp />
  </React.StrictMode>,
)
