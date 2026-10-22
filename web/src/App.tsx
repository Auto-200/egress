import { Navigate, Route, Routes } from 'react-router-dom';
import { AuthProvider, RequireAuth } from '@/auth';
import { Layout } from '@/components/layout';
import { AuditPage } from '@/pages/audit';
import { DashboardPage } from '@/pages/dashboard';
import { LinesPage } from '@/pages/lines';
import { LoginPage } from '@/pages/login';
import { NodesPage } from '@/pages/nodes';
import { RegionsPage } from '@/pages/regions';

export default function App() {
  return (
    <AuthProvider>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route
          element={
            <RequireAuth>
              <Layout />
            </RequireAuth>
          }
        >
          <Route index element={<DashboardPage />} />
          <Route path="lines" element={<LinesPage />} />
          <Route path="nodes" element={<NodesPage />} />
          <Route path="regions" element={<RegionsPage />} />
          <Route path="audit" element={<AuditPage />} />
        </Route>
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </AuthProvider>
  );
}
