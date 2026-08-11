import { useSettingsStore } from '../../../store/useSettingsStore'
import Section from '../../shared/Section'
import Row from '../../shared/Row'
import Toggle from '../../shared/Toggle'

export default function StartupTab() {
  const settings = useSettingsStore(s => s.settings)
  const setLaunchAtLogin = useSettingsStore(s => s.setLaunchAtLogin)
  const setStartMinimized = useSettingsStore(s => s.setStartMinimized)

  return (
    <div>
      <Section title="起動時の動作" />
      <Row label="Windows起動時に自動起動" desc="ログイン時にFlexFindを常駐開始します">
        <Toggle value={settings.launchAtLogin} onChange={v => void setLaunchAtLogin(v)} />
      </Row>
      <Row label="起動時にトレイに最小化" desc="OFFにすると起動時に検索オーバーレイを一度表示します">
        <Toggle value={settings.startMinimizedToTray} onChange={v => void setStartMinimized(v)} />
      </Row>
    </div>
  )
}
