import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Globe, Github, MessageCircle, Info } from "lucide-react";

export function AboutPage() {
  // In a real application, you might fetch the version from an API
  const version = "1.0.0"; // Placeholder for now

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      {/* Version Information */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
              <Info className="h-5 w-5 text-blue-600" />
            </div>
            Version Information
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">
                CleverCoffee Version
              </span>
              <span className="font-mono text-sm bg-muted px-2 py-1 rounded">
                {version}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">UI Version</span>
              <span className="font-mono text-sm bg-muted px-2 py-1 rounded">
                2.0.0-beta
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Community & Resources */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-green-500/10">
              <MessageCircle className="h-5 w-5 text-green-600" />
            </div>
            Community & Resources
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <a
              href="https://clevercoffee.de/"
              className="block"
              target="_blank"
              rel="noopener noreferrer"
            >
              <div className="flex flex-col items-center justify-center p-6 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 h-32">
                <Globe className="h-8 w-8 mb-3 text-blue-600" />
                <p className="font-medium text-center">Project Website</p>
                <p className="text-xs text-muted-foreground text-center mt-1">
                  clevercoffee.de
                </p>
              </div>
            </a>
            <a
              href="https://github.com/rancilio-pid/clevercoffee"
              className="block"
              target="_blank"
              rel="noopener noreferrer"
            >
              <div className="flex flex-col items-center justify-center p-6 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 h-32">
                <Github className="h-8 w-8 mb-3 text-gray-700 dark:text-gray-300" />
                <p className="font-medium text-center">GitHub Repository</p>
                <p className="text-xs text-muted-foreground text-center mt-1">
                  Source code & issues
                </p>
              </div>
            </a>
            <a
              href="https://discord.gg/Kq5RFznuU4"
              className="block"
              target="_blank"
              rel="noopener noreferrer"
            >
              <div className="flex flex-col items-center justify-center p-6 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 h-32">
                <MessageCircle className="h-8 w-8 mb-3 text-purple-600" />
                <p className="font-medium text-center">Discord Community</p>
                <p className="text-xs text-muted-foreground text-center mt-1">
                  Get help & support
                </p>
              </div>
            </a>
          </div>
        </CardContent>
      </Card>

      {/* Project Information */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-orange-500/10">
              <Info className="h-5 w-5 text-orange-600" />
            </div>
            About CleverCoffee
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-muted-foreground">
            CleverCoffee is an open-source PID controller firmware for espresso
            machines, providing precise temperature control and advanced brewing
            features. This modern web interface offers real-time monitoring,
            parameter configuration, and system management.
          </p>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 pt-4">
            <div className="space-y-2">
              <h4 className="font-medium">Key Features</h4>
              <ul className="text-sm text-muted-foreground space-y-1">
                <li>• PID temperature control</li>
                <li>• Real-time monitoring</li>
                <li>• Parameter management</li>
                <li>• System configuration</li>
              </ul>
            </div>
            <div className="space-y-2">
              <h4 className="font-medium">Support</h4>
              <ul className="text-sm text-muted-foreground space-y-1">
                <li>• Community Discord server</li>
                <li>• GitHub issues & discussions</li>
                <li>• Project documentation</li>
                <li>• Open source contributions</li>
              </ul>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
