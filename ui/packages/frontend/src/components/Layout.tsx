import { Outlet, Link, useLocation } from "react-router-dom";
import { ModeToggle } from "./mode-toggle";
import { LiveStatusIndicator } from "./LiveStatusIndicator";
import { StandbyNotification } from "./StandbyNotification";
import { cn } from "@/lib/utils";
import { useState } from "react";
import { Menu, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { basePathWithoutTrailingSlash } from "@/lib/config";

export function Layout() {
  const location = useLocation();
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false);

  const isActive = (path: string) => {
    if (path === "/") {
      return location.pathname === "/";
    }
    return location.pathname.startsWith(path);
  };

  const navigationItems = [
    { path: "/", label: "Home" },
    { path: "/config/behavior", label: "Configuration" },
    { path: "/system", label: "System" },
    { path: "/about", label: "About" },
  ];

  const closeMobileMenu = () => setIsMobileMenuOpen(false);

  return (
    <div className="flex min-h-screen w-full flex-col">
      <StandbyNotification />
      <header className="sticky top-0 flex h-16 items-center gap-4 border-b bg-background px-4 md:px-6 justify-between z-40">
        {/* Desktop Navigation */}
        <nav className="hidden flex-col gap-6 text-lg font-medium md:flex md:flex-row md:items-center md:gap-5 md:text-sm lg:gap-6">
          <Link
            to="/"
            className="flex items-center gap-2 text-lg font-semibold md:text-base"
          >
            <img
              src={`${basePathWithoutTrailingSlash}/logo.png`}
              className="h-6 w-6"
              alt="Logo"
            />
            <span className="sr-only">CleverCoffee</span>
          </Link>
          {navigationItems.map((item) => (
            <Link
              key={item.path}
              to={
                item.path === "/settings/behavior"
                  ? "/settings/behavior"
                  : item.path
              }
              className={cn(
                "transition-colors hover:text-foreground font-medium",
                isActive(
                  item.path === "/settings/behavior" ? "/settings" : item.path
                )
                  ? "text-foreground"
                  : "text-muted-foreground"
              )}
            >
              {item.label}
            </Link>
          ))}
        </nav>

        {/* Mobile Navigation */}
        <div className="flex items-center gap-4 md:hidden">
          <Link
            to="/"
            className="flex items-center gap-2 text-lg font-semibold"
          >
            <img
              src={`${basePathWithoutTrailingSlash}/logo.png`}
              className="h-6 w-6"
              alt="Logo"
            />
            <span className="sr-only">CleverCoffee</span>
          </Link>
        </div>

        <div className="flex items-center gap-2">
          <LiveStatusIndicator />
          <ModeToggle />

          {/* Mobile Menu Button */}
          <Button
            variant="ghost"
            size="icon"
            className="md:hidden"
            onClick={() => setIsMobileMenuOpen(!isMobileMenuOpen)}
            aria-label="Toggle menu"
          >
            {isMobileMenuOpen ? (
              <X className="h-5 w-5" />
            ) : (
              <Menu className="h-5 w-5" />
            )}
          </Button>
        </div>
      </header>

      {/* Mobile Menu Overlay */}
      {isMobileMenuOpen && (
        <div className="fixed inset-0 top-16 z-40 bg-background/80 backdrop-blur-sm md:hidden">
          <nav className="flex flex-col gap-2 p-4 bg-background border-b shadow-lg">
            {navigationItems.map((item) => (
              <Link
                key={item.path}
                to={
                  item.path === "/settings/behavior"
                    ? "/settings/behavior"
                    : item.path
                }
                className={cn(
                  "flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground",
                  isActive(
                    item.path === "/settings/behavior" ? "/settings" : item.path
                  )
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground"
                )}
                onClick={closeMobileMenu}
              >
                {item.label}
              </Link>
            ))}
          </nav>
        </div>
      )}

      <main>
        <Outlet />
      </main>
    </div>
  );
}
