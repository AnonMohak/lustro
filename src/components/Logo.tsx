import logoUrl from "../assets/logo.svg";

/** The one Lustro logo. Replace src/assets/logo.svg to change it everywhere. */
export function Logo({ size = 18 }: { size?: number }) {
  return <img className="logo" src={logoUrl} width={size} height={size} alt="" draggable={false} />;
}
