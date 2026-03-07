"use client";

import { createContext, useContext, useState, type ReactNode } from "react";

interface AccountContextType {
  customerId: string;
  setCustomerId: (id: string) => void;
}

const AccountContext = createContext<AccountContextType>({
  customerId: "",
  setCustomerId: () => {},
});

export function useAccount() {
  return useContext(AccountContext);
}

export function AccountProvider({ children }: { children: ReactNode }) {
  // Default to the PlutoCare sub-account from env or hardcode fallback
  const [customerId, setCustomerId] = useState(
    process.env.NEXT_PUBLIC_DEFAULT_CUSTOMER_ID ?? "7616751962"
  );

  return (
    <AccountContext.Provider value={{ customerId, setCustomerId }}>
      {children}
    </AccountContext.Provider>
  );
}
