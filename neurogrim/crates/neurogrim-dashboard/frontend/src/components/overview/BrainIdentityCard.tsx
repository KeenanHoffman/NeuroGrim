import type { OverviewResponse } from "@bindings/OverviewResponse";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from "@/components/ui/card";

interface BrainIdentityCardProps {
  overview: OverviewResponse;
}

/**
 * Header card naming the Brain + summarizing its declared shape:
 * domain count, weighted/advisory split, federation peer count.
 * The "what is this Brain" answer at-a-glance.
 */
export function BrainIdentityCard({ overview }: BrainIdentityCardProps) {
  const {
    project_label,
    project_root,
    domain_count,
    weighted_count,
    advisory_count,
    federation_peer_count,
  } = overview;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-3xl">{project_label}</CardTitle>
        <CardDescription className="font-mono text-xs">
          {project_root}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline">
            {domain_count} domain{domain_count === 1 ? "" : "s"}
          </Badge>
          {weighted_count > 0 && (
            <Badge variant="secondary">{weighted_count} weighted</Badge>
          )}
          {advisory_count > 0 && (
            <Badge variant="secondary">{advisory_count} advisory</Badge>
          )}
          {federation_peer_count > 0 && (
            <Badge variant="outline">
              {federation_peer_count} federation peer
              {federation_peer_count === 1 ? "" : "s"}
            </Badge>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
