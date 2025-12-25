import { createRootRoute, Outlet } from '@tanstack/react-router'
import { MantineProvider } from '@mantine/core'
import { NatsProvider } from '../hooks/useNats'

export const Route = createRootRoute({
  component: () => (
    <MantineProvider defaultColorScheme="dark">
      <NatsProvider>
        <Outlet />
      </NatsProvider>
    </MantineProvider>
  ),
})

