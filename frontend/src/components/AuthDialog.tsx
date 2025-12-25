import { useState, useEffect, useRef } from 'react'
import { Modal, Button, Grid, Text, Box, Paper, useMantineTheme } from '@mantine/core'
import { IconBackspace, IconTrash } from '@tabler/icons-react'
import { useNats } from '../hooks/useNats'

const DIGITS = [
  ['1', '2', '3'],
  ['4', '5', '6'],
  ['7', '8', '9'],
  ['', '0', ''],
]

interface AuthDialogProps {
  opened: boolean
  onClose: () => void
}

export function AuthDialog({ opened, onClose }: AuthDialogProps) {
  const [pin, setPin] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const submittingRef = useRef(false)
  const theme = useMantineTheme()
  const { authenticate } = useNats()

  const handleSubmit = async (pinToSubmit?: string) => {
    const pinValue = pinToSubmit ?? pin
    
    // Prevent multiple simultaneous submissions
    if (loading || submittingRef.current) return
    
    // Validate PIN format (4 digits)
    if (!/^\d{4}$/.test(pinValue)) {
      setError('PIN must be 4 digits')
      return
    }

    submittingRef.current = true
    setLoading(true)
    setError(null)

    try {
      await authenticate(pinValue)
      // Success - clear PIN and close modal
      setPin('')
      setError(null)
      onClose()
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Authentication failed'
      setError(errorMessage)
      setPin('') // Clear PIN on error
    } finally {
      setLoading(false)
      submittingRef.current = false
    }
  }

  const handleClose = () => {
    setPin('')
    setError(null)
    onClose()
  }

  const handleNumberInput = (num: string) => {
    // Prevent input during loading or submission
    if (loading || submittingRef.current) return
    
    setPin(prevPin => {
      if (prevPin.length < 4) {
        const newPin = prevPin + num
        
        // Clear any previous errors when entering a new digit
        setError(null)
        
        // Auto-submit when 4 digits are entered
        if (newPin.length === 4) {
          // Use setTimeout to ensure state update completes before submitting
          setTimeout(() => {
            handleSubmit(newPin)
          }, 0)
        }
        
        return newPin
      }
      return prevPin
    })
  }

  const handleNumberClick = (num: string) => {
    handleNumberInput(num)
  }

  const handleBackspace = () => {
    // Prevent backspace during loading or submission
    if (loading || submittingRef.current) return
    
    setPin(prevPin => {
      if (prevPin.length > 0) {
        setError(null)
        return prevPin.slice(0, -1)
      }
      return prevPin
    })
  }

  const handleClear = () => {
    // Prevent clear during loading or submission
    if (loading || submittingRef.current) return
    
    setPin('')
    setError(null)
  }

  // Handle keyboard input
  useEffect(() => {
    if (!opened) return

    const handleKeyDown = (event: KeyboardEvent) => {
      // Prevent default if we're handling the key
      if (event.key >= '0' && event.key <= '9') {
        event.preventDefault()
        handleNumberInput(event.key)
      } else if (event.key === 'Backspace' || event.key === 'Delete') {
        event.preventDefault()
        handleBackspace()
      } else if (event.key === 'Escape') {
        event.preventDefault()
        handleClose()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [opened, loading])

  // Reset PIN when modal opens
  useEffect(() => {
    if (opened) {
      setPin('')
      setError(null)
      submittingRef.current = false
    }
  }, [opened])


  return (
    <Modal
      opened={opened}
      onClose={handleClose}
      title="Enter Pin"
      centered
    >
      {/* PIN Display */}
      <Box mb="md" pt="md">
        <Box
          style={{
            display: 'flex',
            gap: '8px',
            justifyContent: 'center',
            marginBottom: '8px',
          }}
        >
          {[0, 1, 2, 3].map((index) => (
            <Box
              key={index}
              style={{
                width: '48px',
                height: '48px',
                border: '2px solid',
                borderColor: pin.length > index 
                  ? theme.colors.blue[6] 
                  : theme.colors.dark[4],
                borderRadius: '8px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                fontSize: '20px',
                fontWeight: 600,
                color: pin.length > index ? theme.colors.blue[6] : theme.colors.dark[2],
                backgroundColor: pin.length > index 
                  ? theme.colors.blue[9] 
                  : theme.colors.dark[7],
                transition: 'all 0.2s',
              }}
            >
              {pin[index] ? '•' : ''}
            </Box>
          ))}
        </Box>
      </Box>

      {/* Number Pad */}
      <Box mb="md" pt="md">
        <Grid gutter="xs">
          {DIGITS.map((row, rowIndex) => (
            <Grid.Col key={rowIndex} span={12}>
              <Grid gutter="xs">
                {row.map((num, colIndex) => {
                  // Special handling for empty cells and the 0 button
                  if (num === '') {
                    // First empty cell in last row - backspace button
                    if (rowIndex === 3 && colIndex === 0) {
                      return (
                        <Grid.Col key={`${rowIndex}-${colIndex}`} span={4}>
                          <Button
                            fullWidth
                            size="lg"
                            variant="filled"
                            color="red"
                            onClick={handleBackspace}
                            disabled={pin.length === 0 || loading}
                            style={{ height: '56px' }}
                          >
                            <IconBackspace size={24} />
                          </Button>
                        </Grid.Col>
                      )
                    }
                    // Last empty cell in last row - clear button
                    if (rowIndex === 3 && colIndex === 2) {
                      return (
                        <Grid.Col key={`${rowIndex}-${colIndex}`} span={4}>
                          <Button
                            fullWidth
                            size="lg"
                            variant="filled"
                            color="red"
                            onClick={handleClear}
                            disabled={pin.length === 0 || loading}
                            style={{ height: '56px' }}
                          >
                            <IconTrash size={24} />
                          </Button>
                        </Grid.Col>
                      )
                    }
                    return <Grid.Col key={`${rowIndex}-${colIndex}`} span={4} />
                  }
                  
                  return (
                    <Grid.Col key={`${rowIndex}-${colIndex}`} span={4}>
                      <Button
                        fullWidth
                        size="lg"
                        variant="filled"
                        onClick={() => handleNumberClick(num)}
                        disabled={pin.length >= 4 || loading}
                        style={{ height: '56px', fontSize: '24px', fontWeight: 600 }}
                      >
                        {num}
                      </Button>
                    </Grid.Col>
                  )
                })}
              </Grid>
            </Grid.Col>
          ))}
        </Grid>
      </Box>

      {/* Error Panel */}
      <Paper
        p="md"
        withBorder
        style={{
          backgroundColor: error ? theme.colors.red[9] : 'transparent',
          borderColor: error ? theme.colors.red[6] : 'transparent',
          minHeight: '52px',
          display: 'flex',
          alignItems: 'center',
          opacity: error ? 1 : 0,
          transition: 'opacity 0.2s',
        }}
      >
        <Text c="red.1" size="sm" fw={500}>
          {error || '\u00A0'}
        </Text>
      </Paper>
    </Modal>
  )
}

