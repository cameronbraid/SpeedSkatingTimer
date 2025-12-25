import { ActionIcon, AppShell, Button, Group, Popover, Switch, Tooltip, useMantineTheme } from '@mantine/core'
import { IconCircleDot, IconCircleX, IconLock, IconLockOpen, IconSettings } from '@tabler/icons-react'
import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { useNats } from '../hooks/useNats'
import { useStopwatch } from '../hooks/useStopwatch'
import { AuthDialog } from './AuthDialog'
import { Duration } from './Duration'
import './StopwatchPage.scss'
import { TickingDuration } from './TickingDuration'

export function StopwatchPage() {
  const [authDialogOpened, setAuthDialogOpened] = useState(false)
  const theme = useMantineTheme()
  const { authenticated, logout } = useNats()
  const {
    state,
    running,
    lagMs,
    laps,
    currentLapTimeNs,
    currentLapId,
    connected,
    arm,
    unarm,
    reset,
  } = useStopwatch()

  return (
    <AppShell>
      <AuthDialog opened={authDialogOpened} onClose={() => setAuthDialogOpened(false)} />
      <AppShell.Header py="4">
        <Group h="100%" px="md" justify="space-between">
          <Group gap="md">
            <Switch
              checked={state != 'Disarmed'}
              onChange={(event) => {
                if (event.currentTarget.checked) {
                  arm()
                } else {
                  unarm()
                }
              }}
              label={state === 'Disarmed' ? 'Disarmed' : 'Armed'}
              disabled={!authenticated}
            />
          </Group>
          {authenticated && (
            <Button
              onClick={reset}
              variant="light"
              color="red"
              size="xs"
              style={{ visibility: state === 'Disarmed' ? 'hidden' : 'visible' }}
            >
              Reset
            </Button>
          )}
          <Group gap="sm">
            {authenticated && (
              <Tooltip label="Hardware Setup">
                <ActionIcon
                  component={Link}
                  to="/hardware-setup"
                  variant="subtle"
                  color="gray"
                >
                  <IconSettings size={20} />
                </ActionIcon>
              </Tooltip>
            )}
            <Popover
              width={200}
              position="bottom"
              withArrow
              shadow="md"
            >
              <Popover.Target>
                <span style={{ display: 'inline-flex', alignItems: 'center', cursor: 'pointer' }}>
                  {connected ? (
                    <IconCircleDot size={20} color={theme.colors.green[6]} />
                  ) : (
                    <IconCircleX size={20} color={theme.colors.red[6]} />
                  )}
                </span>
              </Popover.Target>
              <Popover.Dropdown>
                {connected
                  ? lagMs > 0
                    ? `Connected (${lagMs.toFixed(0)}ms lag)`
                    : 'Connected'
                  : 'Disconnected'}
              </Popover.Dropdown>
            </Popover>
            {authenticated ? (
              <Tooltip label="Click to logout and reconnect">
                <ActionIcon
                  variant="subtle"
                  color="blue"
                  onClick={logout}
                >
                  <IconLock size={20} />
                </ActionIcon>
              </Tooltip>
            ) : (
              <Tooltip label="Click to authenticate">
                <ActionIcon
                  variant="subtle"
                  color="orange"
                  onClick={() => setAuthDialogOpened(true)}
                >
                  <IconLockOpen size={20} />
                </ActionIcon>
              </Tooltip>
            )}
          </Group>
        </Group>
      </AppShell.Header>

      <AppShell.Main>
        <div className="StopwatchPage">
          <div className="DurationContainer">
            <TickingDuration
              startTimeNs={currentLapTimeNs}
              lapId={currentLapId}
              mode="big"
              lagMs={lagMs}
              running={running}
            />
          </div>

          {laps.length > 0 && (
            <div className="LapsContainer">
              {laps.map((lap) => (
                <div key={`${lap.runId}-${lap.lap}`} className="LapClock">
                  <Duration
                    duration={lap.lapTimeNs / 1_000_000}
                    mode="mini"
                    previousLap={true}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      </AppShell.Main>
    </AppShell>
  )
}

