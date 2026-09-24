import { DashboardPage } from "./features/dashboard/dashboard-page";
import { OnboardingFlow } from "./features/onboarding/onboarding-flow";

export default function App() {
  return <OnboardingFlow ready={(user, signOut, recheck) => <DashboardPage user={user} onStorageProblem={recheck} onSignOut={signOut} />} />;
}
