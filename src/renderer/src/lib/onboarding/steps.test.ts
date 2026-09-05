import { describe, expect, it } from 'vitest'
import { getFeatureSteps } from './steps'

describe('0.13 feature onboarding', () => {
  it('keeps five upgrade steps while introducing the CI/CD entry point', () => {
    const steps = getFeatureSteps().filter((step) => step.introducedIn === '0.13.0')

    expect(steps).toHaveLength(5)
    const projectServices = steps.find((step) => step.id === 'task-tracker-panel')
    expect(projectServices?.description).toContain('Enable CI/CD in Sidebar settings')
    expect(projectServices?.description).toContain('TeamCity jobs or GitHub Actions workflows')
  })
})
