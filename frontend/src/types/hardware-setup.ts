// Hardware Setup Types

export interface PinState {
  pin: number
  name: string
  active: boolean // true = beam broken/closed, false = beam clear/open
}

export interface HardwareSetupState {
  armed: boolean
  pins: PinState[]
}
