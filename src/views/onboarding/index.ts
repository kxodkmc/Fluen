/**
 * Onboarding module — public exports.
 *
 * Only the top-level view component and its types are re-exported here.
 * Internal step / helper components stay private to the module so that
 * the public API surface remains minimal.
 */
export { default as OnboardingView } from './OnboardingView.vue';
export * from './types';
export * from './constants';
export { useOnboardingState } from './composables/useOnboardingState';
