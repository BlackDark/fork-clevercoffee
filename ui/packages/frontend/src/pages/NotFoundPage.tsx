import { useEffect } from "react";
import { Link } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { AlertCircle, Home, ArrowLeft } from "lucide-react";

export function NotFoundPage() {
  useEffect(() => {
    // Set document title for 404 page
    document.title = "404 - Page Not Found | CleverCoffee";

    // Add meta tag for robots (prevent indexing of 404 pages)
    const metaRobots = document.createElement("meta");
    metaRobots.name = "robots";
    metaRobots.content = "noindex, nofollow";
    document.head.appendChild(metaRobots);

    // Log 404 for debugging/analytics
    console.warn(`404 - Page not found: ${window.location.pathname}`);

    // Cleanup on unmount
    return () => {
      document.title = "CleverCoffee";
      document.head.removeChild(metaRobots);
    };
  }, []);

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      <div className="flex items-center justify-center min-h-[60vh]">
        <Card className="w-full max-w-lg">
          <CardHeader className="text-center">
            <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-lg bg-red-500/10">
              <AlertCircle className="h-8 w-8 text-red-600" />
            </div>
            <CardTitle className="text-3xl font-bold">404</CardTitle>
            <p className="text-lg text-muted-foreground">Page Not Found</p>
          </CardHeader>
          <CardContent className="text-center space-y-6">
            <div className="space-y-2">
              <h2 className="text-xl font-semibold">
                Oops! This page doesn't exist
              </h2>
              <p className="text-muted-foreground">
                The page you're looking for might have been moved, deleted, or
                you may have entered an incorrect URL.
              </p>
            </div>

            <div className="flex flex-col sm:flex-row gap-3 justify-center">
              <Button asChild>
                <Link to="/">
                  <Home className="mr-2 h-4 w-4" />
                  Go to Home
                </Link>
              </Button>
              <Button variant="outline" onClick={() => window.history.back()}>
                <ArrowLeft className="mr-2 h-4 w-4" />
                Go Back
              </Button>
            </div>

            <div className="pt-4 border-t">
              <p className="text-sm text-muted-foreground mb-3">
                Available pages:
              </p>
              <div className="flex flex-wrap gap-2 justify-center">
                <Link to="/" className="text-sm text-primary hover:underline">
                  Home
                </Link>
                <span className="text-muted-foreground">•</span>
                <Link
                  to="/settings/general"
                  className="text-sm text-primary hover:underline"
                >
                  Parameters
                </Link>
                <span className="text-muted-foreground">•</span>
                <Link
                  to="/system"
                  className="text-sm text-primary hover:underline"
                >
                  System
                </Link>
                <span className="text-muted-foreground">•</span>
                <Link
                  to="/about"
                  className="text-sm text-primary hover:underline"
                >
                  About
                </Link>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
