import {
  ActionIcon,
  AppShell,
  Box,
  Button,
  Group,
  Paper,
  Popover,
  SimpleGrid,
  Switch,
  Text,
  Tooltip,
  useMantineTheme,
} from '@mantine/core'
import {
  IconCircleDot,
  IconCircleX,
  IconLock,
  IconLockOpen,
  IconArrowLeft,
} from '@tabler/icons-react'
import { useState } from 'react'
import { Link } from '@tanstack/react-router'
import { useNats } from '../hooks/useNats'
import { useHardwareSetup } from '../hooks/useHardwareSetup'
import { useLatency } from '../hooks/useLatency'
import { AuthDialog } from './AuthDialog'
import type { PinState } from '../types/hardware-setup'

function PinIndicator({ pin, name, active }: PinState) {
  const theme = useMantineTheme()

  return (
    <Paper
      p="md"
      radius="md"
      style={{
        backgroundColor: active
          ? theme.colors.red[9]
          : theme.colors.green[9],
        border: `2px solid ${active ? theme.colors.red[6] : theme.colors.green[6]}`,
        textAlign: 'center',
        transition: 'all 0.15s ease',
      }}
    >
      <Text size="lg" fw={700} c={active ? 'red.1' : 'green.1'}>
        {name}
      </Text>
      <Text size="xs" c={active ? 'red.3' : 'green.3'}>
        GPIO {pin}
      </Text>
      <Text size="sm" fw={600} c={active ? 'red.2' : 'green.2'} mt={8}>
        {active ? 'BLOCKED' : 'CLEAR'}
      </Text>
    </Paper>
  )
}

export function HardwareSetupPage() {
  const [authDialogOpened, setAuthDialogOpened] = useState(false)
  const theme = useMantineTheme()
  const { authenticated, logout } = useNats()
  const { lagMs } = useLatency()
  const {
    armed,
    pins,
    loading,
    connected,
    arm,
    disarm,
  } = useHardwareSetup()

  return (
    <AppShell header={{ height: 60 }}>
      <AuthDialog opened={authDialogOpened} onClose={() => setAuthDialogOpened(false)} />
      <AppShell.Header py="4">
        <Group h="100%" px="md" justify="space-between">
          <Group gap="md">
            <Tooltip label="Back to Stopwatch">
              <ActionIcon
                component={Link}
                to="/"
                variant="subtle"
                color="gray"
              >
                <IconArrowLeft size={20} />
              </ActionIcon>
            </Tooltip>
            <Text fw={600}>Hardware Setup</Text>
            <Switch
              checked={armed}
              onChange={() => {
                if (armed) {
                  disarm()
                } else {
                  arm()
                }
              }}
              label={armed ? 'Armed' : 'Disarmed'}
              disabled={!authenticated || loading}
            />
          </Group>
          <Group gap="sm">
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
        <Box p="xl">
          {!authenticated && (
            <Paper p="lg" mb="xl" withBorder style={{ backgroundColor: theme.colors.yellow[9] }}>
              <Text c="yellow.1" fw={500}>
                Authentication required to arm/disarm hardware setup mode.
              </Text>
              <Button
                mt="sm"
                variant="filled"
                color="yellow"
                onClick={() => setAuthDialogOpened(true)}
              >
                Authenticate
              </Button>
            </Paper>
          )}

          {!armed && (
            <Paper p="lg" mb="xl" withBorder>
              <Text c="dimmed">
                Arm the hardware setup to see live GPIO pin states.
                This is used for aligning IR sensors.
              </Text>
            </Paper>
          )}

          {armed && pins.length === 0 && (
            <Paper p="lg" mb="xl" withBorder>
              <Text c="dimmed">
                No GPIO pins configured. Make sure the server is running with GPIO trigger enabled.
              </Text>
            </Paper>
          )}

          {armed && pins.length > 0 && (
            <SimpleGrid
              cols={{ base: 1, xs: 2, sm: 3, md: 4 }}
              spacing="lg"
            >
              {pins.map((pin) => (
                <PinIndicator key={pin.pin} {...pin} />
              ))}
            </SimpleGrid>
          )}
        </Box>
      </AppShell.Main>
    </AppShell>
  )
}
