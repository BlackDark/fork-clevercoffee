import type { PageLoad } from "./$types";

export const load: PageLoad = ({ params }) => {
  return {
    filter: params.filter,
  };
};
