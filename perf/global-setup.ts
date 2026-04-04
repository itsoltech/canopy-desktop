import { mkdirSync } from 'fs'
import { resolve } from 'path'

export default function globalSetup(): void {
  mkdirSync(resolve(__dirname, 'results'), { recursive: true })
}
