import { createFileRoute } from '@tanstack/react-router'
import { HardwareSetupPage } from '../components/HardwareSetupPage'

export const Route = createFileRoute('/hardware-setup')({
  component: HardwareSetupPage,
})
