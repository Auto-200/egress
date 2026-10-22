import { NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom';
import {
  Globe2,
  LayoutDashboard,
  LogOut,
  Route,
  ScrollText,
  Server,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { useAuth } from '@/auth';

const navItems = [
  { to: '/', label: '总览', icon: LayoutDashboard, end: true },
  { to: '/lines', label: '线路', icon: Route },
  { to: '/nodes', label: '节点', icon: Server },
  { to: '/regions', label: '地区', icon: Globe2 },
  { to: '/audit', label: '审计', icon: ScrollText },
];

const titles: Record<string, { title: string; subtitle: string }> = {
  '/': { title: '总览', subtitle: '线路健康、节点状态与最近变更' },
  '/lines': { title: '线路', subtitle: '线路策略、节点分配与健康' },
  '/nodes': { title: '节点', subtitle: '地区出口节点、ACL 与健康上报' },
  '/regions': { title: '地区', subtitle: '地区与线路的基础目录' },
  '/audit': { title: '审计', subtitle: '控制面的最近变更' },
};

export function Layout() {
  const { user, signOut } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const meta = titles[location.pathname] ?? titles['/'];

  const handleSignOut = async () => {
    await signOut();
    navigate('/login', { replace: true });
  };

  return (
    <div className="min-h-screen bg-background">
      <aside className="fixed inset-y-0 left-0 z-30 hidden w-60 flex-col border-r bg-card md:flex">
        <div className="flex h-16 items-center gap-3 border-b px-5">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
            <Route className="h-4 w-4" />
          </div>
          <div>
            <div className="text-sm font-semibold tracking-tight">Egress</div>
            <div className="text-[11px] text-muted-foreground">Control Plane</div>
          </div>
        </div>
        <nav className="flex-1 space-y-1 p-3">
          {navItems.map((item) => {
            const Icon = item.icon;
            return (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.end}
                className={({ isActive }) =>
                  cn(
                    'flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground',
                    isActive && 'bg-accent text-accent-foreground hover:bg-accent',
                  )
                }
              >
                <Icon className="h-4 w-4" />
                {item.label}
              </NavLink>
            );
          })}
        </nav>
        <div className="border-t p-3">
          <div className="mb-2 px-2 text-xs text-muted-foreground">登录为 {user}</div>
          <Button variant="ghost" size="sm" className="w-full justify-start" onClick={handleSignOut}>
            <LogOut className="h-4 w-4" />
            退出登录
          </Button>
        </div>
      </aside>

      <div className="md:pl-60">
        <header className="sticky top-0 z-20 border-b bg-background/90 backdrop-blur">
          <div className="flex h-16 items-center justify-between px-4 md:px-8">
            <div>
              <h1 className="text-lg font-semibold tracking-tight">{meta.title}</h1>
              <p className="text-xs text-muted-foreground">{meta.subtitle}</p>
            </div>
            <Button
              variant="ghost"
              size="sm"
              className="md:hidden"
              onClick={handleSignOut}
              aria-label="退出登录"
            >
              <LogOut className="h-4 w-4" />
            </Button>
          </div>
          <div className="flex gap-1 overflow-auto border-t px-3 py-2 md:hidden">
            {navItems.map((item) => {
              const Icon = item.icon;
              return (
                <NavLink
                  key={item.to}
                  to={item.to}
                  end={item.end}
                  className={({ isActive }) =>
                    cn(
                      'flex shrink-0 items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs text-muted-foreground',
                      isActive && 'bg-accent text-accent-foreground',
                    )
                  }
                >
                  <Icon className="h-3.5 w-3.5" />
                  {item.label}
                </NavLink>
              );
            })}
          </div>
        </header>
        <main className="p-4 md:p-8">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
